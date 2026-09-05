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
//! Usage (from cella_lib/):
//!   cargo run --release --example wildfire_smc -- <scenario_dir> <members> <open|assim> <out.json>
//! Env: SMC_BETA (10), SMC_SIGMA (0.2), SMC_IMMIGRANTS (0), SMC_SEED (0),
//!      SMC_WIND_ROT_DEG (0), SMC_ASSIM_EVERY (1),
//!      SMC_PRIOR=path.json (a JSON array of genes replacing the default list),
//!      SMC_CONTAIN=1 (add the containment genes `contain_a`/`contain_b`, so
//!      the driver draws a containment once a day — the E28 operator),
//!      SMC_TAU_OFF=1 (drop the `tau_days` gene so containment is the only stop).
//!
//! Default genes (the E25 prior): `model.p0` log-uniform 0.08–0.6,
//! `model.burn_duration` 5–20, `tau_days` log-uniform 2–100 days,
//! `wind_scale` 0–1.5.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cella_lib::config::CellaConfig;
use cella_lib::explore::driver::Forcing;
use cella_lib::explore::metrics::{brier, iou, mean_sd};
use cella_lib::wildfire::driver::{
    FORCING_HOURS, FORCING_WIND_FROM, FORCING_WIND_SPEED, GENE_CONTAIN_A, GENE_CONTAIN_B,
    GENE_TAU_DAYS, GENE_WIND_SCALE, STATE_CONTAINED,
};
use cella_lib::{CellType, Ensemble, EnsembleConfig, GeneSpec, ParamValue, WildfireDriver};
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

    let ens_cfg = EnsembleConfig {
        members,
        seed: envf("SMC_SEED", 0.0) as u64,
        genes: genes.clone(),
        track: vec!["Burning".into(), "BurnedOut".into()],
        beta: envf("SMC_BETA", 10.0),
        sigma: envf("SMC_SIGMA", 0.2),
        immigrants: envf("SMC_IMMIGRANTS", 0.0),
        driver: Some(Box::new(WildfireDriver {
            // One containment draw per simulated day.
            steps_per_day: (24.0 * sc.steps_per_hour).round().max(1.0) as u64,
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
