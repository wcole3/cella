//! Ensemble forecasting for the wildfire model (validation E24 / E25 / E28),
//! built on the library's model-agnostic [`cella_lib::Ensemble`] plus the
//! [`cella_lib::wildfire::WildfireDriver`] — the same two pieces any other
//! model would use.
//!
//! `open` mode — plain Monte Carlo: `M` members with genes drawn from the
//! ranges below, run independently; the per-cell burn probability is scored
//! as a probabilistic forecast (Brier, consensus IoU, best-threshold IoU)
//! beside the deterministic nulls (persistence, area-matched radial).
//!
//! `assim` mode — after each observation the ensemble is scored, then
//! [`Ensemble::assimilate`] resamples, mutates and admits immigrants, and the
//! members keep simulating. Every score at t_k is a forecast from the state
//! assimilated at t_{k-1}: the mask at t_k is never seen before it is scored.
//!
//! `evolve` mode (validation E36) — fit first: a genetic algorithm
//! ([`cella_lib::Evolution`] with the same driver) searches the genes for the
//! settings whose single run best matches the first `SMC_FIT_DAYS` observed
//! perimeters (mean IoU over those days). The winner is then run forward as
//! an `open` ensemble (every member = the fitted genes, its own seed) and
//! scored on every observation, so the days after the fit window are honest
//! forecasts and directly comparable with `assim` on the same days.
//!
//! `map` mode (validation E37) — MAP-Elites illumination of the spread genes
//! (`model.p0`, `model.burn_duration`, `wind_scale`): no objective, two
//! behaviour axes (growth of the burned area, elongation of its shape) over
//! `SMC_MAP_DAYS` days of the scenario's weather. The report is the archive:
//! which shapes and sizes the model can produce at all, next to the observed
//! perimeter's own growth and elongation on the same days.
//!
//! Usage (from cella_lib/):
//!   cargo run --release --example wildfire_smc -- <scenario_dir> <members> <open|assim|evolve|map> <out.json>
//! Env: SMC_BETA (10), SMC_SIGMA (0.2), SMC_IMMIGRANTS (0), SMC_CROSSOVER (0), SMC_SEED (0),
//!      SMC_IMM_RESET=1 (immigrants start uncontained, with p0 from their own genome),
//!      SMC_WIND_ROT_DEG (0), SMC_ASSIM_EVERY (1),
//!      SMC_PRIOR=path.json (a JSON array of genes replacing the default list),
//!      SMC_CONTAIN=1 (add the containment genes `contain_a`/`contain_b`, so
//!      the driver draws a containment once a day — the E28 operator),
//!      SMC_TAU_OFF=1 (drop the `tau_days` gene so containment is the only stop),
//!      SMC_FIT_DAYS (3), SMC_GENERATIONS (20 evolve / 30 map), SMC_POP (24 evolve /
//!      32 map batch), SMC_REPEATS (2), SMC_MAP_DAYS (5).
//!
//! Default genes (the E25 prior): `model.p0` log-uniform 0.08–0.6,
//! `model.burn_duration` 5–20, `tau_days` log-uniform 2–100 days,
//! `wind_scale` 0–1.5.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use std::sync::Arc;

use cella_lib::config::CellaConfig;
use cella_lib::explore::archive::{ArchiveReport, DescriptorSpec};
use cella_lib::explore::driver::Forcing;
use cella_lib::explore::metrics::{Fitness, When, brier, elongation, fraction, iou, mean_sd};
use cella_lib::explore::{Search, Sim};
use cella_lib::wildfire::driver::{
    FORCING_HOURS, FORCING_WIND_FROM, FORCING_WIND_SPEED, GENE_CONTAIN_A, GENE_CONTAIN_B,
    GENE_TAU_DAYS, GENE_WIND_SCALE, STATE_CONTAINED, WeatherWindow, WildfireDriver,
};
use cella_lib::wildfire::wind_toward_grid_deg;
use cella_lib::{CellType, Ensemble, EnsembleConfig, GeneSpec, ParamValue};
use cella_lib::{Evolution, EvolveConfig, Grid2D, Metric, Rule2D};
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
    from_deg: f64,
}

#[derive(Deserialize)]
struct Truth {
    format_version: u32,
    observed_at: Vec<f64>,
    arrival_hours: Vec<f64>,
}

#[derive(Serialize)]
struct ObsScore {
    hours: f64,
    obs_burned: u64,
    mean_member_iou: f64,
    best_member_iou: f64,
    consensus_iou: f64,
    union_iou: f64,
    best_threshold_iou: f64,
    best_threshold: f64,
    area_ratio_mean: f64,
    brier_ensemble: f64,
    brier_radial: f64,
    brier_persistence: f64,
    radial_iou: f64,
    persistence_iou: f64,
    /// The Ellipse null (ERA5 wind variant, E41), reported beside the
    /// Circle in every ensemble mode so future tables carry it for free.
    ellipse_iou: f64,
    brier_ellipse: f64,
    ess: f64,
    p0_mean: f64,
    p0_std: f64,
    tau_mean: f64,
    tau_std: f64,
    dur_mean: f64,
    wind_scale_mean: f64,
    contained_fraction: f64,
}

#[derive(Serialize)]
struct Report {
    scenario: String,
    mode: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the short git commit hash `wildfire_smc` was compiled from
    /// (`"unknown"` if `git` wasn't available at build time).
    binary_git: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the UTC timestamp `wildfire_smc` was compiled at.
    binary_built_utc: String,
    members: usize,
    beta: f64,
    sigma: f64,
    immigrants: f64,
    assim_every: usize,
    genes: Vec<GeneSpec>,
    scores: Vec<ObsScore>,
    final_consensus_iou: f64,
    final_best_threshold_iou: f64,
    final_radial_iou: f64,
    mean_consensus_iou: f64,
    mean_best_threshold_iou: f64,
    mean_member_iou: f64,
    mean_radial_iou: f64,
    mean_brier_ensemble: f64,
    mean_brier_radial: f64,
    final_genomes: Vec<BTreeMap<String, ParamValue>>,
    /// Present in `evolve` mode: what the fit found before the forecast ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    fit: Option<FitReport>,
}

/// The offline fit of `evolve` mode.
#[derive(Serialize)]
struct FitReport {
    fit_days: usize,
    fit_steps: u64,
    population: usize,
    generations: usize,
    repeats: usize,
    /// Mean IoU over the fit days of the best genome's evaluation runs.
    best_fit_iou: f64,
    best_genome: BTreeMap<String, ParamValue>,
    /// `(generation, best, mean)` per generation.
    log: Vec<(u64, f64, f64)>,
}

/// The `map` mode's report: the archive plus the observed fire's own place
/// in the same descriptor space.
#[derive(Serialize)]
struct MapReport {
    scenario: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the short git commit hash `wildfire_smc` was compiled from
    /// (`"unknown"` if `git` wasn't available at build time).
    binary_git: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the UTC timestamp `wildfire_smc` was compiled at.
    binary_built_utc: String,
    days: u64,
    steps: u64,
    genes: Vec<GeneSpec>,
    generations: usize,
    batch: usize,
    archive: ArchiveReport,
    /// `(hours, growth, elongation)` of the observed burned set at each
    /// observation up to `days`.
    observed: Vec<(f64, f64, f64)>,
    /// `(generation, coverage, qd_score)` per generation.
    log: Vec<(u64, f64, f64)>,
}

/// One observation's scores in `nulls` mode (E41): the deterministic dummy
/// forecasters only, no ensemble. `ellipse_station_*` are `None` when the
/// scenario has no station file.
#[derive(Serialize)]
struct NullObsScore {
    hours: f64,
    obs_burned: u64,
    persistence_iou: f64,
    brier_persistence: f64,
    radial_iou: f64,
    brier_radial: f64,
    ellipse_era5_iou: f64,
    brier_ellipse_era5: f64,
    ellipse_station_iou: Option<f64>,
    brier_ellipse_station: Option<f64>,
    ellipse_era5x3_iou: f64,
    brier_ellipse_era5x3: f64,
    /// Post-hoc control (not pre-registered — TEST_PLAN v1.8 addendum):
    /// same ERA5 wind and LB as `ellipse_era5`, but centred (no front/back
    /// skew). Separates "which way the fire runs" from "how stretched".
    ellipse_era5_centred_iou: f64,
    brier_ellipse_era5_centred: f64,
}

/// The `nulls` mode report (E41): persistence, the Circle and the three
/// Ellipse variants, scored at every observation, with no ensemble at all
/// — cheap enough to run for every fire in a few seconds.
#[derive(Serialize)]
struct NullsReport {
    scenario: String,
    mode: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the short git commit hash `wildfire_smc` was compiled from
    /// (`"unknown"` if `git` wasn't available at build time).
    binary_git: String,
    /// Which build produced this file, so two runs can be told apart:
    /// the UTC timestamp `wildfire_smc` was compiled at.
    binary_built_utc: String,
    /// Whether `station_hourly.json` was found for this scenario; when
    /// `false` every `ellipse_station_*` field above and below is `None`.
    station_available: bool,
    wall_time_secs: f64,
    scores: Vec<NullObsScore>,
    mean_persistence_iou: f64,
    final_persistence_iou: f64,
    mean_radial_iou: f64,
    final_radial_iou: f64,
    mean_ellipse_era5_iou: f64,
    final_ellipse_era5_iou: f64,
    mean_ellipse_station_iou: Option<f64>,
    final_ellipse_station_iou: Option<f64>,
    mean_ellipse_era5x3_iou: f64,
    final_ellipse_era5x3_iou: f64,
    /// Post-hoc control (not pre-registered), same wind/LB as `ellipse_era5`
    /// but centred — see `NullObsScore.ellipse_era5_centred_iou`.
    mean_ellipse_era5_centred_iou: f64,
    final_ellipse_era5_centred_iou: f64,
    mean_brier_persistence: f64,
    mean_brier_radial: f64,
    mean_brier_ellipse_era5: f64,
    mean_brier_ellipse_station: Option<f64>,
    mean_brier_ellipse_era5x3: f64,
    mean_brier_ellipse_era5_centred: f64,
}

/// Fitness for `evolve` mode: IoU against the observed perimeter at each of
/// the fit days, averaged. Sampled every step; steps that are not an
/// observation contribute nothing.
struct ObsFitness {
    /// `(step, hours)` of every observation inside the fit window.
    obs: Vec<(u64, f64)>,
    arrival: Vec<f64>,
    burnt: [CellType; 2],
}

impl Fitness for ObsFitness {
    fn sample(&self, sim: &Sim) -> f64 {
        let step = sim.step_count();
        match self.obs.iter().find(|(s, _)| *s == step) {
            Some((_, hours)) => iou(&sim.mask(&self.burnt), &mask_at(&self.arrival, *hours)),
            None => f64::NAN,
        }
    }

    fn every_step(&self) -> bool {
        true
    }

    fn aggregate(&self, samples: &[f64]) -> f64 {
        let hits: Vec<f64> = samples.iter().copied().filter(|v| v.is_finite()).collect();
        if hits.is_empty() {
            0.0
        } else {
            hits.iter().sum::<f64>() / hits.len() as f64
        }
    }
}

/// A gene pinned to one value, so an `open` ensemble runs one genome with
/// many seeds.
fn pinned(key: &str, value: &ParamValue) -> GeneSpec {
    let v = match value {
        ParamValue::Float(x) => *x,
        ParamValue::Int(i) => *i as f64,
        other => panic!("cannot pin a {other:?} gene"),
    };
    GeneSpec::range(key, v, v)
}

/// The scenario's wind as a driver schedule (hours are absolute).
fn weather_schedule(sc: &Scenario, rot: f64) -> Vec<WeatherWindow> {
    sc.wind
        .iter()
        .map(|w| WeatherWindow {
            hours: w.hours,
            speed_ms: w.speed_ms,
            from_deg: w.from_deg + rot,
        })
        .collect()
}

/// A grid holding just the observed burned set, for the shape metrics.
fn observed_sim(mask: &[bool], w: usize, h: usize) -> Sim {
    let burning = CellType::new("Burning");
    let cells = mask
        .iter()
        .map(|&b| if b { burning } else { CellType::inactive() })
        .collect();
    Sim::D2(Grid2D::new(w, h, 0, cells, Rule2D { subrules: vec![] }))
}

fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn mask_at(arrival: &[f64], t: f64) -> Vec<bool> {
    arrival.iter().map(|&a| a >= 0.0 && a <= t).collect()
}

/// Chamfer distance (3-4 mask) from the seed set; basis of the radial null.
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
                if x > 0 {
                    best = best.min(d[idx(x - 1, y - 1)] + 4);
                }
                if x + 1 < w {
                    best = best.min(d[idx(x + 1, y - 1)] + 4);
                }
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
                if x + 1 < w {
                    best = best.min(d[idx(x + 1, y + 1)] + 4);
                }
                if x > 0 {
                    best = best.min(d[idx(x - 1, y + 1)] + 4);
                }
            }
            d[idx(x, y)] = best;
        }
    }
    d
}

/// One hourly reading from `station_hourly.json` (E41's `ellipse_station`
/// variant). Extra fields in the file (temperature, humidity, provenance)
/// are ignored — serde only pulls out what this struct names.
#[derive(Deserialize)]
struct StationRow {
    hours: f64,
    from_deg: f64,
    speed_ms: f64,
}

/// The station file's shape: a header we don't need plus the hourly rows.
#[derive(Deserialize)]
struct StationLog {
    rows: Vec<StationRow>,
}

/// The vector mean of the station wind over `[start_hours, end_hours)`, as
/// (speed_ms, grid "toward" direction in radians) — the same pair
/// [`grow_ellipse`] wants from the ERA5 schedule.
///
/// "Vector mean" means: turn every hourly reading into a little arrow
/// (length = speed, direction = where it blows toward), add the arrows,
/// then read the length and direction back off the sum. That is different
/// from averaging the speeds and the bearings separately — a wind that is
/// due north for one hour and due south the next averages to *no* wind by
/// vector mean (the arrows cancel), which is physically what happened,
/// while separately-averaged bearings would nonsensically call that
/// "due east or west, who knows". Rotating first (from the compass
/// meteorological bearing to the grid's toward-angle) and averaging after
/// gives the same answer as averaging first and rotating after, because
/// rotation is linear — so we can average directly in grid coordinates.
///
/// Falls back to the single closest row when the window contains none
/// (the campaign's station files are hourly with no gaps, so this should
/// not trigger; it exists so a thinner future log degrades instead of
/// panicking).
fn station_vector_mean(log: &StationLog, start_hours: f64, end_hours: f64) -> (f64, f64) {
    let in_window: Vec<&StationRow> = log
        .rows
        .iter()
        .filter(|r| r.hours >= start_hours - 1e-6 && r.hours < end_hours - 1e-6)
        .collect();
    let rows: Vec<&StationRow> = if in_window.is_empty() {
        let mid = (start_hours + end_hours) / 2.0;
        log.rows
            .iter()
            .min_by(|a, b| {
                (a.hours - mid)
                    .abs()
                    .partial_cmp(&(b.hours - mid).abs())
                    .expect("hours are finite")
            })
            .into_iter()
            .collect()
    } else {
        in_window
    };
    let (mut ux, mut uy) = (0.0, 0.0);
    for r in &rows {
        let toward = wind_toward_grid_deg(r.from_deg).to_radians();
        ux += r.speed_ms * toward.cos();
        uy += r.speed_ms * toward.sin();
    }
    let n = rows.len().max(1) as f64;
    let (mx, my) = (ux / n, uy / n);
    (mx.hypot(my), my.atan2(mx))
}

/// Anderson (1983)'s length-to-breadth ratio of a wind-driven fire ellipse,
/// from the 10 m wind speed `u` in m/s. `LB = 1` is a circle (no wind
/// effect); it grows with speed and is clamped to `[1, 8]` because the raw
/// curve is only fit over the range fires are actually observed at — past
/// that it is extrapolation, not physics.
fn anderson_lb(u: f64) -> f64 {
    let lb = 0.936 * (0.2566 * u).exp() + 0.461 * (-0.1548 * u).exp() - 0.397;
    lb.clamp(1.0, 8.0)
}

/// The travel-time cost of one Dijkstra step `(dx, dy)` (grid cells; the
/// 8-connected offsets, so `1` or `√2`) under a wind blowing *toward*
/// `wind_toward_rad` (0 = +x, turning toward +y — the grid convention
/// [`wind_toward_grid_deg`] produces), with ellipse length-to-breadth
/// ratio `lb`.
///
/// **What an ellipse template is.** A calm fire spreads the same speed in
/// every direction: a circle. A wind-driven fire spreads fastest with the
/// wind, slowest against it, in between on the flanks — an ellipse. Put
/// the ignition point at the ellipse's rear focus (not its centre) and the
/// three textbook rates fall out of two numbers, semi-major axis `a` and
/// semi-minor axis `b` (`b = 1` here; only the *shape* of the ellipse
/// matters, its overall size is decided by the area match, not by `a`/`b`
/// in physical units), with `c = √(a² − b²)` the distance from the centre
/// to a focus:
///
/// - **head rate** (straight downwind, θ = 0): `a + c` — the tip of the
///   ellipse is `a + c` from the focus, the far side of the centre.
/// - **back rate** (straight upwind, θ = 180°): `a − c` — the near tip is
///   only `a − c` from the focus, because the focus sits *inside* that
///   short end, near the fire's own start.
/// - **flank rate** (crosswind, θ = 90°): `b² / a` — the ellipse's
///   half-width as seen from the focus.
///
/// Worked example: wind gives `LB = a = 3` (`b = 1`), so
/// `c = √(9 − 1) ≈ 2.83`. Head rate `a + c ≈ 5.83`, back rate
/// `a − c ≈ 0.17`, flank rate `b²/a ≈ 0.33`: the fire runs about 34× faster
/// downwind than upwind, and about 17× faster downwind than sideways — the
/// long, narrow lobe a real wind-driven fire draws. The general formula
/// used below, `r(θ) = b² / (a − c·cos θ)`, reduces to exactly these three
/// numbers at θ = 0°, 180°, 90° and interpolates between them.
///
/// Growing outward at the *reciprocal* rate — cost, not speed — turns the
/// three rates into three travel times, and Dijkstra over that cost field
/// grows a mask whose boundary is (approximately, on a discrete grid) that
/// same ellipse.
fn ellipse_step_cost(dx: f64, dy: f64, wind_toward_rad: f64, lb: f64) -> f64 {
    let norm = (dx * dx + dy * dy).sqrt();
    let (wx, wy) = (wind_toward_rad.cos(), wind_toward_rad.sin());
    let cos_theta = (dx * wx + dy * wy) / norm;
    let (a, b) = (lb, 1.0f64);
    let c = (a * a - b * b).sqrt();
    let r = b * b / (a - c * cos_theta);
    norm / r
}

/// Post-hoc control for E41 (added after seeing the rear-focus results,
/// not pre-registered — see the TEST_PLAN v1.8 addendum). The rear-focus
/// template above bakes in a front/back skew — `(a + c) / (a − c)` — from
/// LB alone, before any stretch is even visible: at `LB = 1.1` (about
/// Ferguson's peak) that ratio is already ≈ 2.4, just from *which way*
/// round the ellipse is pointed. This control removes that skew: the
/// ignition sits at the ellipse's *centre*, so head and back rates are
/// equal (`a`) and only the flank rate (`1`) differs — `LB` alone can no
/// longer encode "front vs. back", only "long axis vs. short axis". The
/// rate is `r(θ) = a·b / √(b²·cos²θ + a²·sin²θ)` (`a = lb`, `b = 1`):
/// head (θ = 0) and back (θ = 180°) both equal `a`, flank (θ = 90°)
/// equals `b`. Comparing this to [`ellipse_step_cost`]'s result on the
/// same fire separates two different explanations for the same IoU gain:
/// this control winning too means the *stretch* (long vs. short axis)
/// carries signal; only the rear-focus version winning means it was the
/// *sign* (front vs. back) all along, which a symmetric LB template can
/// give away almost for free.
fn centred_ellipse_step_cost(dx: f64, dy: f64, wind_toward_rad: f64, lb: f64) -> f64 {
    let norm = (dx * dx + dy * dy).sqrt();
    let (wx, wy) = (wind_toward_rad.cos(), wind_toward_rad.sin());
    let cos_theta = (dx * wx + dy * wy) / norm;
    let (a, b) = (lb, 1.0f64);
    let r = a * b / (b * b * cos_theta * cos_theta + a * a * (1.0 - cos_theta * cos_theta)).sqrt();
    norm / r
}

/// Grows `mask` outward to `target_area` true cells by Dijkstra over
/// `step_cost(dx, dy) -> cost` (a step's grid offset to its travel-time
/// cost): the accepted set, stopped as soon as its size reaches
/// `target_area`, supersedes the starting mask (every starting cell is at
/// cost 0, so it is always accepted first). Shared by [`grow_ellipse`] and
/// [`grow_ellipse_centred`], which differ only in `step_cost`.
///
/// Deterministic: ties in accumulated cost break on cell index, the lowest
/// index first (`BinaryHeap<Reverse<(cost_bits, idx)>>` — `f64::to_bits`
/// preserves numeric order for the non-negative, finite costs here).
///
/// `target_area` is assumed to be at least the current mask's area — the
/// observed burned area this is matched to only grows between
/// observations — so every seed cell is always among the accepted set;
/// debug builds check this rather than silently returning a mask smaller
/// than `mask` itself.
fn dijkstra_grow(
    mask: &[bool],
    w: usize,
    h: usize,
    target_area: usize,
    step_cost: impl Fn(f64, f64) -> f64,
) -> Vec<bool> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    debug_assert!(
        target_area >= mask.iter().filter(|&&b| b).count(),
        "target_area must not shrink below the current mask (observed area is monotone)"
    );
    const OFFSETS: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    let mut visited = vec![false; mask.len()];
    let mut accepted = 0usize;
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
    for (i, &m) in mask.iter().enumerate() {
        if m {
            heap.push(Reverse((0.0f64.to_bits(), i)));
        }
    }
    while let Some(Reverse((cost_bits, i))) = heap.pop() {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        accepted += 1;
        if accepted >= target_area {
            break;
        }
        let cost = f64::from_bits(cost_bits);
        let (x, y) = (i % w, i / w);
        for &(dx, dy) in &OFFSETS {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let ni = (ny as usize) * w + nx as usize;
            if visited[ni] {
                continue;
            }
            let step = step_cost(f64::from(dx), f64::from(dy));
            heap.push(Reverse(((cost + step).to_bits(), ni)));
        }
    }
    visited
}

/// Grows `mask` by the wind-oriented, rear-focus Ellipse (E41's headline
/// null). `wind_toward_rad`/`lb` set the direction and shape of
/// [`ellipse_step_cost`]; see its docs for what the ellipse template means.
fn grow_ellipse(
    mask: &[bool],
    w: usize,
    h: usize,
    wind_toward_rad: f64,
    lb: f64,
    target_area: usize,
) -> Vec<bool> {
    dijkstra_grow(mask, w, h, target_area, |dx, dy| {
        ellipse_step_cost(dx, dy, wind_toward_rad, lb)
    })
}

/// Grows `mask` by the centred-ellipse control ([`centred_ellipse_step_cost`]
/// — post-hoc, see its docs). Same signature as [`grow_ellipse`].
fn grow_ellipse_centred(
    mask: &[bool],
    w: usize,
    h: usize,
    wind_toward_rad: f64,
    lb: f64,
    target_area: usize,
) -> Vec<bool> {
    dijkstra_grow(mask, w, h, target_area, |dx, dy| {
        centred_ellipse_step_cost(dx, dy, wind_toward_rad, lb)
    })
}

/// The E25 prior as genes.
fn default_genes() -> Vec<GeneSpec> {
    vec![
        GeneSpec::log_range("model.p0", 0.08, 0.6),
        GeneSpec::range("model.burn_duration", 5.0, 20.0),
        GeneSpec::log_range(GENE_TAU_DAYS, 2.0, 100.0),
        GeneSpec::range(GENE_WIND_SCALE, 0.0, 1.5),
    ]
}

/// Mean of a numeric gene over the members, or `fallback` when the gene is
/// not part of this run (e.g. `tau_days` with SMC_TAU_OFF).
fn gene_mean_sd(ens: &Ensemble, key: &str, fallback: f64) -> (f64, f64) {
    match ens.genome_stats(key) {
        Some((mean, sd, _, _)) => (mean, sd),
        None => (fallback, 0.0),
    }
}

fn forcing(hours: f64, speed_ms: f64, from_deg: f64) -> Forcing {
    let mut f = Forcing::new();
    f.insert(FORCING_HOURS.into(), hours);
    f.insert(FORCING_WIND_SPEED.into(), speed_ms);
    f.insert(FORCING_WIND_FROM.into(), from_deg);
    f
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).expect("scenario dir"));
    let members: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(32);
    let mode = args.get(3).cloned().unwrap_or_else(|| "open".into());
    let out = PathBuf::from(
        args.get(4)
            .cloned()
            .unwrap_or_else(|| "smc_report.json".into()),
    );
    let envf = |k: &str, d: f64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let rot = envf("SMC_WIND_ROT_DEG", 0.0);
    let assim_every = envf("SMC_ASSIM_EVERY", 1.0).max(1.0) as usize;
    let mut genes: Vec<GeneSpec> = std::env::var("SMC_PRIOR")
        .ok()
        .map(|p| load(Path::new(&p)))
        .unwrap_or_else(default_genes);
    if envf("SMC_CONTAIN", 0.0) > 0.0 {
        genes.push(GeneSpec::new(GENE_CONTAIN_A));
        genes.push(GeneSpec::new(GENE_CONTAIN_B));
    }
    if envf("SMC_TAU_OFF", 0.0) > 0.0 {
        genes.retain(|g| g.key != GENE_TAU_DAYS);
    }
    let assim = mode == "assim";

    let sc: Scenario = load(&dir.join("scenario.json"));
    let truth: Truth = load(&dir.join("truth.json"));
    let cfg: CellaConfig = load(&dir.join("config.json"));
    assert_eq!(sc.format_version, 2, "unknown scenario format");
    assert_eq!(truth.format_version, 2, "unknown truth format");
    let total = sc.grid.width * sc.grid.height;
    let burnt = [CellType::new("Burning"), CellType::new("BurnedOut")];
    let steps_per_day = (24.0 * sc.steps_per_hour).round().max(1.0) as u64;
    let seed = envf("SMC_SEED", 0.0) as u64;

    if mode == "map" {
        run_map(
            &sc,
            &truth,
            &cfg,
            &out,
            &genes,
            rot,
            steps_per_day,
            seed,
            &burnt,
        );
        return;
    }
    if mode == "nulls" {
        run_nulls(&sc, &truth, &dir, rot, &out);
        return;
    }

    let mut fit = None;
    if mode == "evolve" {
        let (report, best) =
            fit_first_days(&sc, &truth, &cfg, &genes, rot, steps_per_day, seed, &burnt);
        eprintln!(
            "fit over {} days: best mean IoU {:.3}, genome {:?}",
            report.fit_days, report.best_fit_iou, best
        );
        genes = genes
            .iter()
            .map(|g| pinned(&g.key, &best[&g.key]))
            .collect();
        fit = Some(report);
    }

    let ens_cfg = EnsembleConfig {
        members,
        seed,
        genes: genes.clone(),
        track: vec!["Burning".into(), "BurnedOut".into()],
        beta: envf("SMC_BETA", 10.0),
        sigma: envf("SMC_SIGMA", 0.2),
        immigrants: envf("SMC_IMMIGRANTS", 0.0),
        crossover: envf("SMC_CROSSOVER", 0.0),
        immigrant_reset: envf("SMC_IMM_RESET", 0.0) > 0.0,
        driver: Some(Box::new(WildfireDriver {
            // One containment draw per simulated day.
            steps_per_day,
            weather: Vec::new(),
        })),
    };
    let template = cfg
        .build_sim()
        .expect("config builds a grid with its model");
    let mut ens = Ensemble::new(template, &ens_cfg).expect("ensemble builds");

    // Nulls from the ignition set.
    let ignition: Vec<bool> = ens.member_mask(0, &burnt);
    let dist = chamfer_from(&ignition, sc.grid.width, sc.grid.height);
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| (dist[i], i));
    // The Ellipse null (E41, ERA5 variant), grown window by window from its
    // own previous mask the same way the Circle above grows from the
    // ignition set — except its growth is anisotropic and wind direction
    // changes window to window, so (unlike the Circle's fixed `order`) the
    // mask genuinely has to be carried forward rather than recomputed.
    let mut ellipse_mask = ignition.clone();

    eprintln!(
        "{}: {}x{}, {} members, mode {mode}, beta {}, sigma {}, immigrants {}, genes {:?}",
        sc.id,
        sc.grid.width,
        sc.grid.height,
        members,
        ens_cfg.beta,
        ens_cfg.sigma,
        ens_cfg.immigrants,
        genes.iter().map(|g| g.key.as_str()).collect::<Vec<_>>()
    );

    let mut scores: Vec<ObsScore> = Vec::new();
    let mut steps_done = 0u64;
    let mut obs_idx = 1usize;
    for win in sc.wind.windows(2) {
        let (cur, next) = (&win[0], &win[1]);
        ens.set_forcing(forcing(cur.hours, cur.speed_ms, cur.from_deg + rot))
            .expect("driver applies the weather");
        let target = (next.hours * sc.steps_per_hour).round() as u64;
        ens.step_n(target - steps_done).expect("members step");
        steps_done = target;

        if obs_idx < truth.observed_at.len()
            && (truth.observed_at[obs_idx] - next.hours).abs() < 1e-6
        {
            let t = next.hours;
            let obs = mask_at(&truth.arrival_hours, t);
            let obs_n = obs.iter().filter(|&&b| b).count() as u64;
            let prob = ens.state_probability(&burnt);
            let mut ious = Vec::with_capacity(members);
            let mut areas = Vec::with_capacity(members);
            for i in 0..members {
                let mask = ens.member_mask(i, &burnt);
                ious.push(iou(&mask, &obs));
                areas.push(mask.iter().filter(|&&b| b).count() as f64);
            }
            let thr_mask = |q: f32| -> Vec<bool> { prob.iter().map(|&p| p >= q).collect() };
            let consensus_iou = iou(&thr_mask(0.5), &obs);
            let union_iou = iou(&thr_mask(1e-9), &obs);
            let (mut best_thr_iou, mut best_thr) = (0.0, 0.0);
            for k in 1..=9 {
                let q = k as f32 / 10.0;
                let v = iou(&thr_mask(q), &obs);
                if v > best_thr_iou {
                    best_thr_iou = v;
                    best_thr = f64::from(q);
                }
            }
            let mut radial = vec![false; total];
            for &i in order.iter().take(obs_n as usize) {
                radial[i] = true;
            }
            let radial_prob: Vec<f32> = radial.iter().map(|&b| b as u8 as f32).collect();
            let pers_prob: Vec<f32> = ignition.iter().map(|&b| b as u8 as f32).collect();
            let toward = wind_toward_grid_deg(cur.from_deg + rot).to_radians();
            let lb = anderson_lb(cur.speed_ms);
            ellipse_mask = grow_ellipse(
                &ellipse_mask,
                sc.grid.width,
                sc.grid.height,
                toward,
                lb,
                obs_n as usize,
            );
            let ellipse_prob: Vec<f32> = ellipse_mask.iter().map(|&b| b as u8 as f32).collect();
            let max_iou = ious.iter().cloned().fold(0.0, f64::max);
            let w: Vec<f64> = ious
                .iter()
                .map(|&v| (ens_cfg.beta * (v - max_iou)).exp())
                .collect();
            let wsum: f64 = w.iter().sum();
            let ess = wsum * wsum / w.iter().map(|x| x * x).sum::<f64>();
            let (p0m, p0s) = gene_mean_sd(&ens, "model.p0", f64::NAN);
            // 150 is the old "decay off" sentinel the summaries expect when
            // the tau gene is absent.
            let (taum, taus) = gene_mean_sd(&ens, GENE_TAU_DAYS, 150.0);
            let (durm, _) = gene_mean_sd(&ens, "model.burn_duration", f64::NAN);
            let (wsm, _) = gene_mean_sd(&ens, GENE_WIND_SCALE, 1.0);
            let contained_fraction = ens.state_fraction(STATE_CONTAINED);
            let score = ObsScore {
                hours: t,
                obs_burned: obs_n,
                mean_member_iou: mean_sd(&ious).0,
                best_member_iou: max_iou,
                consensus_iou,
                union_iou,
                best_threshold_iou: best_thr_iou,
                best_threshold: best_thr,
                area_ratio_mean: areas.iter().sum::<f64>() / members as f64 / obs_n.max(1) as f64,
                brier_ensemble: brier(&prob, &obs),
                brier_radial: brier(&radial_prob, &obs),
                brier_persistence: brier(&pers_prob, &obs),
                radial_iou: iou(&radial, &obs),
                persistence_iou: iou(&ignition, &obs),
                ellipse_iou: iou(&ellipse_mask, &obs),
                brier_ellipse: brier(&ellipse_prob, &obs),
                ess,
                p0_mean: p0m,
                p0_std: p0s,
                tau_mean: taum,
                tau_std: taus,
                dur_mean: durm,
                wind_scale_mean: wsm,
                contained_fraction,
            };
            eprintln!(
                "  t={t:5.0}h obs {obs_n:7} | member IoU mean {:.3} best {:.3} | consensus {:.3} bestthr {:.3}@{:.1} | radial {:.3} | Brier ens {:.4} radial {:.4} | ESS {:.1} | p0 {:.2}±{:.2} tau {:.1} | contained {:.0}%",
                score.mean_member_iou,
                max_iou,
                consensus_iou,
                best_thr_iou,
                best_thr,
                score.radial_iou,
                score.brier_ensemble,
                score.brier_radial,
                ess,
                p0m,
                p0s,
                taum,
                100.0 * contained_fraction
            );
            scores.push(score);

            // The containment draw for the day just scored ran inside step_n
            // at the day boundary (the driver's period); only learning is left.
            if assim && obs_idx + 1 < truth.observed_at.len() && obs_idx.is_multiple_of(assim_every)
            {
                ens.assimilate(&obs, &burnt)
                    .expect("observation matches the grid");
            }
            obs_idx += 1;
        }
    }

    let n = scores.len() as f64;
    let mean = |f: &dyn Fn(&ObsScore) -> f64| scores.iter().map(f).sum::<f64>() / n;
    let report = Report {
        scenario: sc.id.clone(),
        mode: mode.clone(),
        binary_git: env!("CELLA_GIT_SHA").to_string(),
        binary_built_utc: env!("CELLA_BUILT_UTC").to_string(),
        members,
        beta: ens_cfg.beta,
        sigma: ens_cfg.sigma,
        immigrants: ens_cfg.immigrants,
        assim_every,
        genes,
        final_consensus_iou: scores.last().map_or(0.0, |s| s.consensus_iou),
        final_best_threshold_iou: scores.last().map_or(0.0, |s| s.best_threshold_iou),
        final_radial_iou: scores.last().map_or(0.0, |s| s.radial_iou),
        mean_consensus_iou: mean(&|s| s.consensus_iou),
        mean_best_threshold_iou: mean(&|s| s.best_threshold_iou),
        mean_member_iou: mean(&|s| s.mean_member_iou),
        mean_radial_iou: mean(&|s| s.radial_iou),
        mean_brier_ensemble: mean(&|s| s.brier_ensemble),
        mean_brier_radial: mean(&|s| s.brier_radial),
        final_genomes: ens.genomes(),
        scores,
        fit,
    };
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!(
        "mean consensus IoU {:.3} | best-threshold {:.3} | radial {:.3} | Brier ens {:.4} vs radial {:.4} -> {}",
        report.mean_consensus_iou,
        report.mean_best_threshold_iou,
        report.mean_radial_iou,
        report.mean_brier_ensemble,
        report.mean_brier_radial,
        out.display()
    );
}

/// Observation steps and hours, in order, skipping the ignition (index 0).
fn observation_steps(sc: &Scenario, truth: &Truth) -> Vec<(u64, f64)> {
    truth
        .observed_at
        .iter()
        .skip(1)
        .map(|&h| ((h * sc.steps_per_hour).round() as u64, h))
        .collect()
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// `evolve` mode, step one: a GA over `genes` scored on the first
/// `SMC_FIT_DAYS` observations. Returns the fit report and the best genome.
#[allow(clippy::too_many_arguments)]
fn fit_first_days(
    sc: &Scenario,
    truth: &Truth,
    cfg: &CellaConfig,
    genes: &[GeneSpec],
    rot: f64,
    steps_per_day: u64,
    seed: u64,
    burnt: &[CellType; 2],
) -> (FitReport, BTreeMap<String, ParamValue>) {
    let fit_days = env_usize("SMC_FIT_DAYS", 3).max(1);
    let obs: Vec<(u64, f64)> = observation_steps(sc, truth)
        .into_iter()
        .take(fit_days)
        .collect();
    let fit_steps = obs.last().map_or(steps_per_day, |o| o.0);
    let fitness = ObsFitness {
        obs,
        arrival: truth.arrival_hours.clone(),
        burnt: *burnt,
    };
    let evo_cfg = EvolveConfig {
        population: env_usize("SMC_POP", 24),
        generations: env_usize("SMC_GENERATIONS", 20),
        seed,
        genes: genes.to_vec(),
        objective: None,
        steps: fit_steps,
        repeats: env_usize("SMC_REPEATS", 2),
        driver: Some(Box::new(WildfireDriver {
            steps_per_day,
            weather: weather_schedule(sc, rot),
        })),
        ..EvolveConfig::default()
    };
    let template = cfg
        .build_sim()
        .expect("config builds a grid with its model");
    let mut evo =
        Evolution::with_fitness(template, &evo_cfg, Arc::new(fitness)).expect("evolution builds");
    let mut log = Vec::new();
    evo.run(evo_cfg.generations, |r| {
        eprintln!(
            "  gen {:2} best {:.3} mean {:.3} invalid {}",
            r.generation, r.best, r.mean, r.invalid
        );
        log.push((r.generation, r.best, r.mean));
    });
    let best = evo.best().expect("at least one valid genome");
    let named = evo.named(&best.genome);
    (
        FitReport {
            fit_days,
            fit_steps,
            population: evo_cfg.population,
            generations: evo_cfg.generations,
            repeats: evo_cfg.repeats,
            best_fit_iou: best.fitness,
            best_genome: named.clone(),
            log,
        },
        named,
    )
}

/// `map` mode: MAP-Elites over the spread genes with growth × elongation
/// axes and no objective, plus the observed fire in the same coordinates.
#[allow(clippy::too_many_arguments)]
fn run_map(
    sc: &Scenario,
    truth: &Truth,
    cfg: &CellaConfig,
    out: &Path,
    genes: &[GeneSpec],
    rot: f64,
    steps_per_day: u64,
    seed: u64,
    burnt: &[CellType; 2],
) {
    let days = env_usize("SMC_MAP_DAYS", 5).max(1) as u64;
    let steps = days * steps_per_day;
    let types: Vec<String> = burnt.iter().map(|t| t.as_str().to_string()).collect();
    let batch = env_usize("SMC_POP", 32);
    let evo_cfg = EvolveConfig {
        population: batch,
        generations: env_usize("SMC_GENERATIONS", 30),
        seed,
        genes: genes.to_vec(),
        objective: None,
        search: Search::MapElites {
            batch,
            iso_line: true,
        },
        descriptors: vec![
            // Growth is a share of the whole grid; a real fire over a few
            // days is a few per cent of it, so the natural [-1, 1] would
            // put every run in one bin.
            DescriptorSpec {
                metric: Metric::Growth {
                    types: types.clone(),
                },
                when: When::End,
                range: Some([0.0, env_f64("SMC_MAP_GROWTH_MAX", 0.1)]),
                bins: 20,
            },
            DescriptorSpec {
                metric: Metric::Elongation { types },
                when: When::End,
                range: Some([1.0, 4.0]),
                bins: 20,
            },
        ],
        thumbnails: false,
        steps,
        repeats: 1,
        driver: Some(Box::new(WildfireDriver {
            steps_per_day,
            weather: weather_schedule(sc, rot),
        })),
        ..EvolveConfig::default()
    };
    let template = cfg
        .build_sim()
        .expect("config builds a grid with its model");
    let mut evo = Evolution::new(template, &evo_cfg).expect("evolution builds");
    let mut log = Vec::new();
    evo.run(evo_cfg.generations, |r| {
        if let Some(a) = &r.archive {
            eprintln!(
                "  gen {:2} elites {} coverage {:.2} qd {:.1} invalid {}",
                r.generation, a.elites, a.coverage, a.qd_score, r.invalid
            );
            log.push((r.generation, a.coverage, a.qd_score));
        }
    });
    let archive = evo
        .archive()
        .expect("map mode has an archive")
        .to_report(false);

    // The observed fire in the same coordinates: growth = burned fraction
    // now minus at ignition; elongation of the burned set.
    let burning = CellType::new("Burning");
    let (w, h) = (sc.grid.width, sc.grid.height);
    let start = fraction(
        &observed_sim(&mask_at(&truth.arrival_hours, 0.0), w, h),
        &[burning],
    );
    let observed: Vec<(f64, f64, f64)> = truth
        .observed_at
        .iter()
        .skip(1)
        .filter(|&&hh| hh <= days as f64 * 24.0 + 1e-6)
        .map(|&hh| {
            let sim = observed_sim(&mask_at(&truth.arrival_hours, hh), w, h);
            (
                hh,
                fraction(&sim, &[burning]) - start,
                elongation(&sim, &[burning]),
            )
        })
        .collect();
    let report = MapReport {
        scenario: sc.id.clone(),
        binary_git: env!("CELLA_GIT_SHA").to_string(),
        binary_built_utc: env!("CELLA_BUILT_UTC").to_string(),
        days,
        steps,
        genes: genes.to_vec(),
        generations: evo_cfg.generations,
        batch,
        archive,
        observed,
        log,
    };
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!(
        "archive: {} elites, coverage {:.2}; observed (hours, growth, elongation) {:?} -> {}",
        report.archive.stats.elites,
        report.archive.stats.coverage,
        report.observed,
        out.display()
    );
}

/// `nulls` mode (validation E41): no ensemble, just the deterministic
/// dummy forecasters — persistence, the Circle and the three Ellipse
/// variants — scored at every observation. This is the cheap, honest
/// first check of whether the *wind inputs we have* (ERA5 daily, or the
/// station log) carry any shape signal at all, before spending any more
/// effort on the fire model's kernel.
///
/// The Circle and the Ellipse both start from the ignition mask
/// (`truth.json`'s t = 0 perimeter — the same set `config.json`'s initial
/// state was built from, so this needs no `CellaConfig` or simulation) and
/// grow window by window. The wind for a window is the scenario's ERA5
/// entry that starts it (`ellipse_era5`, `ellipse_era5x3` — same entry,
/// speed × 3), or the vector mean of the station log's hourly rows that
/// fall inside it (`ellipse_station`, `None` when there is no station
/// file for this fire).
fn run_nulls(sc: &Scenario, truth: &Truth, dir: &Path, rot: f64, out: &Path) {
    let start = std::time::Instant::now();
    let (w, h) = (sc.grid.width, sc.grid.height);
    let total = w * h;
    let ignition = mask_at(&truth.arrival_hours, 0.0);
    let dist = chamfer_from(&ignition, w, h);
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| (dist[i], i));

    let station_path = std::env::var("SMC_STATION_WIND")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dir.join("station_hourly.json"));
    let station: Option<StationLog> = station_path.exists().then(|| load(&station_path));
    let station_available = station.is_some();

    let mut ellipse_era5 = ignition.clone();
    let mut ellipse_station = ignition.clone();
    let mut ellipse_era5x3 = ignition.clone();
    let mut ellipse_era5_centred = ignition.clone();

    let mut scores: Vec<NullObsScore> = Vec::new();
    let mut obs_idx = 1usize;
    for win in sc.wind.windows(2) {
        let (cur, next) = (&win[0], &win[1]);
        if obs_idx >= truth.observed_at.len()
            || (truth.observed_at[obs_idx] - next.hours).abs() >= 1e-6
        {
            continue;
        }
        let t = next.hours;
        let obs = mask_at(&truth.arrival_hours, t);
        let obs_n = obs.iter().filter(|&&b| b).count();

        let mut radial = vec![false; total];
        for &i in order.iter().take(obs_n) {
            radial[i] = true;
        }

        let toward_era5 = wind_toward_grid_deg(cur.from_deg + rot).to_radians();
        ellipse_era5 = grow_ellipse(
            &ellipse_era5,
            w,
            h,
            toward_era5,
            anderson_lb(cur.speed_ms),
            obs_n,
        );
        ellipse_era5x3 = grow_ellipse(
            &ellipse_era5x3,
            w,
            h,
            toward_era5,
            anderson_lb(cur.speed_ms * 3.0),
            obs_n,
        );
        // Post-hoc control (not pre-registered): same wind and LB as
        // ellipse_era5, but centred — see centred_ellipse_step_cost's docs.
        ellipse_era5_centred = grow_ellipse_centred(
            &ellipse_era5_centred,
            w,
            h,
            toward_era5,
            anderson_lb(cur.speed_ms),
            obs_n,
        );
        let (ellipse_station_iou, brier_ellipse_station) = match &station {
            Some(log) => {
                let (speed, toward) = station_vector_mean(log, cur.hours, next.hours);
                ellipse_station =
                    grow_ellipse(&ellipse_station, w, h, toward, anderson_lb(speed), obs_n);
                let prob: Vec<f32> = ellipse_station.iter().map(|&b| b as u8 as f32).collect();
                (Some(iou(&ellipse_station, &obs)), Some(brier(&prob, &obs)))
            }
            None => (None, None),
        };

        let prob_of = |m: &[bool]| -> Vec<f32> { m.iter().map(|&b| b as u8 as f32).collect() };
        scores.push(NullObsScore {
            hours: t,
            obs_burned: obs_n as u64,
            persistence_iou: iou(&ignition, &obs),
            brier_persistence: brier(&prob_of(&ignition), &obs),
            radial_iou: iou(&radial, &obs),
            brier_radial: brier(&prob_of(&radial), &obs),
            ellipse_era5_iou: iou(&ellipse_era5, &obs),
            brier_ellipse_era5: brier(&prob_of(&ellipse_era5), &obs),
            ellipse_station_iou,
            brier_ellipse_station,
            ellipse_era5x3_iou: iou(&ellipse_era5x3, &obs),
            brier_ellipse_era5x3: brier(&prob_of(&ellipse_era5x3), &obs),
            ellipse_era5_centred_iou: iou(&ellipse_era5_centred, &obs),
            brier_ellipse_era5_centred: brier(&prob_of(&ellipse_era5_centred), &obs),
        });
        eprintln!(
            "  t={t:5.0}h obs {obs_n:7} | persistence {:.3} radial {:.3} | ellipse era5 {:.3} station {} era5x3 {:.3} era5_centred {:.3}",
            iou(&ignition, &obs),
            iou(&radial, &obs),
            scores.last().unwrap().ellipse_era5_iou,
            scores
                .last()
                .unwrap()
                .ellipse_station_iou
                .map_or("  n/a".to_string(), |v| format!("{v:.3}")),
            scores.last().unwrap().ellipse_era5x3_iou,
            scores.last().unwrap().ellipse_era5_centred_iou,
        );
        obs_idx += 1;
    }

    let n = scores.len() as f64;
    let mean = |f: &dyn Fn(&NullObsScore) -> f64| scores.iter().map(f).sum::<f64>() / n;
    let mean_opt = |f: &dyn Fn(&NullObsScore) -> Option<f64>| -> Option<f64> {
        if station_available {
            Some(scores.iter().filter_map(f).sum::<f64>() / n)
        } else {
            None
        }
    };
    let report = NullsReport {
        scenario: sc.id.clone(),
        mode: "nulls".to_string(),
        binary_git: env!("CELLA_GIT_SHA").to_string(),
        binary_built_utc: env!("CELLA_BUILT_UTC").to_string(),
        station_available,
        wall_time_secs: start.elapsed().as_secs_f64(),
        mean_persistence_iou: mean(&|s| s.persistence_iou),
        final_persistence_iou: scores.last().map_or(0.0, |s| s.persistence_iou),
        mean_radial_iou: mean(&|s| s.radial_iou),
        final_radial_iou: scores.last().map_or(0.0, |s| s.radial_iou),
        mean_ellipse_era5_iou: mean(&|s| s.ellipse_era5_iou),
        final_ellipse_era5_iou: scores.last().map_or(0.0, |s| s.ellipse_era5_iou),
        mean_ellipse_station_iou: mean_opt(&|s| s.ellipse_station_iou),
        final_ellipse_station_iou: scores.last().and_then(|s| s.ellipse_station_iou),
        mean_ellipse_era5x3_iou: mean(&|s| s.ellipse_era5x3_iou),
        final_ellipse_era5x3_iou: scores.last().map_or(0.0, |s| s.ellipse_era5x3_iou),
        mean_ellipse_era5_centred_iou: mean(&|s| s.ellipse_era5_centred_iou),
        final_ellipse_era5_centred_iou: scores.last().map_or(0.0, |s| s.ellipse_era5_centred_iou),
        mean_brier_persistence: mean(&|s| s.brier_persistence),
        mean_brier_radial: mean(&|s| s.brier_radial),
        mean_brier_ellipse_era5: mean(&|s| s.brier_ellipse_era5),
        mean_brier_ellipse_station: mean_opt(&|s| s.brier_ellipse_station),
        mean_brier_ellipse_era5x3: mean(&|s| s.brier_ellipse_era5x3),
        mean_brier_ellipse_era5_centred: mean(&|s| s.brier_ellipse_era5_centred),
        scores,
    };
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!(
        "{}: nulls in {:.1}s | mean IoU persistence {:.3} radial {:.3} ellipse_era5 {:.3} ellipse_era5x3 {:.3} ellipse_era5_centred {:.3} ellipse_station {} -> {}",
        sc.id,
        report.wall_time_secs,
        report.mean_persistence_iou,
        report.mean_radial_iou,
        report.mean_ellipse_era5_iou,
        report.mean_ellipse_era5x3_iou,
        report.mean_ellipse_era5_centred_iou,
        report
            .mean_ellipse_station_iou
            .map_or("n/a".to_string(), |v| format!("{v:.3}")),
        out.display()
    );
}

#[cfg(test)]
mod ellipse_null_tests {
    use super::*;

    fn seed_mask(w: usize, h: usize, cx: usize, cy: usize) -> Vec<bool> {
        let mut m = vec![false; w * h];
        m[cy * w + cx] = true;
        m
    }

    #[test]
    fn anderson_lb_is_one_at_zero_wind_and_clamped_above_eight() {
        // The Anderson curve is fit to equal 1.0 (a circle) at U = 0 by
        // construction: 0.936 + 0.461 - 0.397 = 1.0 exactly.
        assert!((anderson_lb(0.0) - 1.0).abs() < 1e-9);
        // Way past the fit range, the clamp keeps the ellipse from
        // running away to a sliver.
        assert!((anderson_lb(100.0) - 8.0).abs() < 1e-9);
        assert!(anderson_lb(5.0) > 1.0 && anderson_lb(5.0) < 8.0);
    }

    #[test]
    fn calm_wind_reproduces_the_chamfer_disc() {
        // LB = 1 makes ellipse_step_cost isotropic (r(theta) = 1 for every
        // theta), so growing by Dijkstra should trace out the same disc
        // the chamfer-distance Circle null does.
        let (w, h) = (200, 200);
        let seed = seed_mask(w, h, 100, 100);
        let target = 3000; // ~radius 31 cells, well clear of the grid edge.
        let dist = chamfer_from(&seed, w, h);
        let mut order: Vec<usize> = (0..w * h).collect();
        order.sort_by_key(|&i| (dist[i], i));
        let mut circle = vec![false; w * h];
        for &i in order.iter().take(target) {
            circle[i] = true;
        }
        let ellipse = grow_ellipse(&seed, w, h, 0.0, 1.0, target);
        let score = iou(&ellipse, &circle);
        assert!(score >= 0.95, "calm-wind IoU with the chamfer disc {score}");
    }

    #[test]
    fn west_wind_stretches_the_grown_set_toward_positive_x() {
        // wind_toward_grid_deg(270) = 0: a west wind blows toward +x. With
        // LB = 3 the ellipse should read as clearly elongated along +x: at
        // least twice as far right of the seed as it reaches up or down.
        let (w, h) = (200, 200);
        let (cx, cy) = (60, 100);
        let seed = seed_mask(w, h, cx, cy);
        let toward = wind_toward_grid_deg(270.0).to_radians();
        let grown = grow_ellipse(&seed, w, h, toward, 3.0, 4000);

        let mut x_extent_right = 0i64;
        let mut y_extent = 0i64;
        for (i, &b) in grown.iter().enumerate() {
            if !b {
                continue;
            }
            let (x, y) = (i % w, i / w);
            x_extent_right = x_extent_right.max(x as i64 - cx as i64);
            y_extent = y_extent.max((y as i64 - cy as i64).abs());
        }
        assert!(
            x_extent_right as f64 >= 2.0 * y_extent as f64,
            "x-extent right {x_extent_right} should be >= 2x the y-extent {y_extent}"
        );
    }

    #[test]
    fn centred_ellipse_has_no_front_back_skew() {
        // The rear-focus ellipse (grow_ellipse) is asymmetric front to
        // back by construction; the centred control must not be. Same
        // wind toward +x, same LB = 3: left extent and right extent
        // should come out within 10% of each other.
        let (w, h) = (200, 200);
        let (cx, cy) = (100, 100);
        let seed = seed_mask(w, h, cx, cy);
        let grown = grow_ellipse_centred(&seed, w, h, 0.0, 3.0, 4000);

        let (mut left, mut right) = (0i64, 0i64);
        for (i, &b) in grown.iter().enumerate() {
            if !b {
                continue;
            }
            let x = (i % w) as i64;
            right = right.max(x - cx as i64);
            left = left.max(cx as i64 - x);
        }
        let (left, right) = (left as f64, right as f64);
        assert!(
            (left - right).abs() <= 0.10 * right.max(left),
            "left extent {left} should be within 10% of right extent {right}"
        );
    }

    #[test]
    fn grow_ellipse_matches_the_target_area_exactly() {
        let (w, h) = (120, 120);
        let seed = seed_mask(w, h, 60, 60);
        for &target in &[1usize, 500, 5000] {
            let grown = grow_ellipse(&seed, w, h, 1.2, 2.5, target);
            let count = grown.iter().filter(|&&b| b).count();
            assert_eq!(count, target.max(1), "target area {target}");
        }
    }
}
