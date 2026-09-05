//! Ensemble forecasting for the wildfire model (validation E24 / E25 / E28),
//! built on the library's model-agnostic [`cella_lib::Ensemble`] plus the
//! [`cella_lib::WildfireDriver`] — the same two pieces any other model would
//! use.
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
use cella_lib::wildfire::driver::WeatherWindow;
use cella_lib::wildfire::driver::{
    FORCING_HOURS, FORCING_WIND_FROM, FORCING_WIND_SPEED, GENE_CONTAIN_A, GENE_CONTAIN_B,
    GENE_TAU_DAYS, GENE_WIND_SCALE, STATE_CONTAINED,
};
use cella_lib::{CellType, Ensemble, EnsembleConfig, GeneSpec, ParamValue, WildfireDriver};
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
