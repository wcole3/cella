//! Ensemble forecasting for the wildfire model (validation E24 / E25), built
//! on the library's [`cella_lib::WildfireEnsemble`].
//!
//! `open` mode — plain Monte Carlo: `M` members from the prior, run
//! independently; the per-cell burn probability is scored as a probabilistic
//! forecast (Brier, consensus IoU, best-threshold IoU) beside the
//! deterministic nulls (persistence, area-matched radial).
//!
//! `assim` mode — after each observation the ensemble is scored, then
//! [`WildfireEnsemble::assimilate`] resamples, mutates and admits immigrants,
//! and the members keep simulating. Every score at t_k is a forecast from the
//! state assimilated at t_{k-1}: the mask at t_k is never seen before it is
//! scored.
//!
//! Usage (from cella_lib/):
//!   cargo run --release --example wildfire_smc -- <scenario_dir> <members> <open|assim> <out.json>
//! Env: SMC_BETA (10), SMC_SIGMA (0.2), SMC_IMMIGRANTS (0), SMC_SEED (0),
//!      SMC_WIND_ROT_DEG (0), SMC_ASSIM_EVERY (1), SMC_PRIOR=path.json,
//!      SMC_CONTAIN=1 (enable the containment-probability operator with its
//!      default prior, applied at every observation time = once a day),
//!      SMC_TAU_OFF=1 (fix the decay off so containment is the only stop).

use std::path::{Path, PathBuf};

use cella_lib::config::CellaConfig;
use cella_lib::ensemble::ContainmentPrior;
use cella_lib::{CellType, EnsembleConfig, MemberParams, WildfireEnsemble, WildfirePrior};
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
    prior: WildfirePrior,
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
    final_params: Vec<MemberParams>,
}

fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn mask_at(arrival: &[f64], t: f64) -> Vec<bool> {
    arrival.iter().map(|&a| a >= 0.0 && a <= t).collect()
}

fn iou(a: &[bool], b: &[bool]) -> f64 {
    cella_lib::ensemble::iou(a, b)
}

fn brier(prob: &[f32], obs: &[bool]) -> f64 {
    prob.iter()
        .zip(obs)
        .map(|(&p, &o)| (f64::from(p) - if o { 1.0 } else { 0.0 }).powi(2))
        .sum::<f64>()
        / prob.len() as f64
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

fn mean_std(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    let s = (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n).sqrt();
    (m, s)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).expect("scenario dir"));
    let members: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(32);
    let mode = args.get(3).cloned().unwrap_or_else(|| "open".into());
    let out = PathBuf::from(args.get(4).cloned().unwrap_or_else(|| "smc_report.json".into()));
    let envf = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let rot = envf("SMC_WIND_ROT_DEG", 0.0);
    let assim_every = envf("SMC_ASSIM_EVERY", 1.0).max(1.0) as usize;
    let mut prior: WildfirePrior = std::env::var("SMC_PRIOR")
        .ok()
        .map(|p| load(Path::new(&p)))
        .unwrap_or_default();
    if envf("SMC_CONTAIN", 0.0) > 0.0 {
        prior.containment = Some(ContainmentPrior::default());
    }
    if envf("SMC_TAU_OFF", 0.0) > 0.0 {
        prior.tau_days = [150.0, 150.0];
    }
    let ens_cfg = EnsembleConfig {
        members,
        seed: envf("SMC_SEED", 0.0) as u64,
        prior,
        beta: envf("SMC_BETA", 10.0),
        sigma: envf("SMC_SIGMA", 0.2),
        immigrants: envf("SMC_IMMIGRANTS", 0.0),
    };
    let assim = mode == "assim";

    let sc: Scenario = load(&dir.join("scenario.json"));
    let truth: Truth = load(&dir.join("truth.json"));
    let cfg: CellaConfig = load(&dir.join("config.json"));
    assert_eq!(sc.format_version, 2, "unknown scenario format");
    assert_eq!(truth.format_version, 2, "unknown truth format");
    let total = sc.grid.width * sc.grid.height;
    let burning = CellType::new("Burning");
    let burned = CellType::new("BurnedOut");

    let mut ens = WildfireEnsemble::from_config(&cfg, &ens_cfg).expect("ensemble builds");

    // Nulls from the ignition set.
    let ignition: Vec<bool> = {
        let g = ens.grids().next().unwrap();
        (0..total)
            .map(|i| {
                let t = g.cell_type(i);
                t == burning || t == burned
            })
            .collect()
    };
    let dist = chamfer_from(&ignition, sc.grid.width, sc.grid.height);
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| (dist[i], i));

    eprintln!(
        "{}: {}x{}, {} members, mode {mode}, beta {}, sigma {}, immigrants {}",
        sc.id, sc.grid.width, sc.grid.height, members, ens_cfg.beta, ens_cfg.sigma, ens_cfg.immigrants
    );

    let mut scores: Vec<ObsScore> = Vec::new();
    let mut steps_done = 0u64;
    let mut obs_idx = 1usize;
    for win in sc.wind.windows(2) {
        let (cur, next) = (&win[0], &win[1]);
        ens.set_weather(cur.hours, cur.speed_ms, cur.from_deg + rot);
        let target = (next.hours * sc.steps_per_hour).round() as u64;
        ens.step_n(target - steps_done);
        steps_done = target;

        if obs_idx < truth.observed_at.len() && (truth.observed_at[obs_idx] - next.hours).abs() < 1e-6 {
            let t = next.hours;
            let obs = mask_at(&truth.arrival_hours, t);
            let obs_n = obs.iter().filter(|&&b| b).count() as u64;
            let prob = ens.burn_probability();
            let mut ious = Vec::with_capacity(members);
            let mut areas = Vec::with_capacity(members);
            for i in 0..members {
                let mask = ens.member_burned(i);
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
            let w: Vec<f64> = ious.iter().map(|&v| (ens_cfg.beta * (v - max_iou)).exp()).collect();
            let wsum: f64 = w.iter().sum();
            let ess = wsum * wsum / w.iter().map(|x| x * x).sum::<f64>();
            let params = ens.params();
            let (p0m, p0s) = mean_std(&params.iter().map(|p| p.p0).collect::<Vec<_>>());
            let (taum, taus) = mean_std(&params.iter().map(|p| p.tau_days.min(150.0)).collect::<Vec<_>>());
            let (durm, _) = mean_std(&params.iter().map(|p| p.burn_duration as f64).collect::<Vec<_>>());
            let (wsm, _) = mean_std(&params.iter().map(|p| p.wind_scale).collect::<Vec<_>>());
            let score = ObsScore {
                hours: t,
                obs_burned: obs_n,
                mean_member_iou: ious.iter().sum::<f64>() / members as f64,
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
                contained_fraction: ens.contained_fraction(),
            };
            eprintln!(
                "  t={t:5.0}h obs {obs_n:7} | member IoU mean {:.3} best {:.3} | consensus {:.3} bestthr {:.3}@{:.1} | radial {:.3} | Brier ens {:.4} radial {:.4} | ESS {:.1} | p0 {:.2}±{:.2} tau {:.1} | contained {:.0}%",
                score.mean_member_iou, max_iou, consensus_iou, best_thr_iou, best_thr, score.radial_iou,
                score.brier_ensemble, score.brier_radial, ess, p0m, p0s, taum, 100.0 * ens.contained_fraction()
            );
            scores.push(score);

            // Containment draw for the day just scored (state for tomorrow).
            ens.end_of_day();
            if assim && obs_idx + 1 < truth.observed_at.len() && obs_idx.is_multiple_of(assim_every) {
                ens.assimilate(&obs).expect("observation matches the grid");
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
        prior: ens_cfg.prior.clone(),
        final_consensus_iou: scores.last().map_or(0.0, |s| s.consensus_iou),
        final_best_threshold_iou: scores.last().map_or(0.0, |s| s.best_threshold_iou),
        final_radial_iou: scores.last().map_or(0.0, |s| s.radial_iou),
        mean_consensus_iou: mean(&|s| s.consensus_iou),
        mean_best_threshold_iou: mean(&|s| s.best_threshold_iou),
        mean_member_iou: mean(&|s| s.mean_member_iou),
        mean_radial_iou: mean(&|s| s.radial_iou),
        mean_brier_ensemble: mean(&|s| s.brier_ensemble),
        mean_brier_radial: mean(&|s| s.brier_radial),
        final_params: ens.params(),
        scores,
    };
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!(
        "mean consensus IoU {:.3} | best-threshold {:.3} | radial {:.3} | Brier ens {:.4} vs radial {:.4} -> {}",
        report.mean_consensus_iou, report.mean_best_threshold_iou, report.mean_radial_iou,
        report.mean_brier_ensemble, report.mean_brier_radial, out.display()
    );
}
