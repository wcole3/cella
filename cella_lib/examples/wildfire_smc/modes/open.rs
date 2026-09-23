//! `open` mode — plain Monte Carlo: `M` members with genes drawn from the
//! ranges below, run independently; the per-cell burn probability is scored
//! as a probabilistic forecast (Brier, consensus IoU, best-threshold IoU)
//! beside the deterministic nulls (persistence, area-matched radial).
//!
//! `assim` mode — after each observation the ensemble is scored, then
//! [`Ensemble::assimilate`] resamples, mutates and admits immigrants, and
//! the members keep simulating (see [`super::assim::maybe_assimilate`]).
//! Every score at t_k is a forecast from the state assimilated at t_{k-1}:
//! the mask at t_k is never seen before it is scored.
//!
//! `evolve` mode (validation E36) runs this same loop for its forecast
//! half, after [`super::evolve::fit_first_days`] has pinned the genes to
//! the fit's winner — `open`, `assim` and `evolve` all end up here with
//! only `mode`/`genes`/`fit` differing between them.

use std::collections::BTreeMap;
use std::path::Path;

use cella_lib::config::CellaConfig;
use cella_lib::explore::driver::Forcing;
use cella_lib::explore::metrics::{brier, iou, mean_sd};
use cella_lib::wildfire::driver::{
    FORCING_HOURS, FORCING_WIND_FROM, FORCING_WIND_SPEED, GENE_TAU_DAYS, GENE_WIND_SCALE,
    STATE_CONTAINED, WildfireDriver,
};
use cella_lib::wildfire::wind_toward_grid_deg;
use cella_lib::{CellType, Ensemble, EnsembleConfig, GeneSpec, ParamValue};
use serde::Serialize;

use crate::knobs::Knobs;
use crate::modes::assim::maybe_assimilate;
use crate::modes::evolve::FitReport;
use crate::nulls::{anderson_lb, chamfer_from, grow_ellipse, radial_mask_from};
use crate::report::{provenance, write_json};
use crate::score::ObsScore;
use crate::{Scenario, Truth, mask_at};

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
    /// Means over every window with a lagged null (k >= 1, see `ObsScore`);
    /// `0.0` if the series never had one (a scenario with a single
    /// observation).
    mean_lagged_persistence_iou: f64,
    mean_brier_lagged_persistence: f64,
    mean_lagged_circle_iou: f64,
    mean_brier_lagged_circle: f64,
    final_genomes: Vec<BTreeMap<String, ParamValue>>,
    /// Present in `evolve` mode: what the fit found before the forecast ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    fit: Option<FitReport>,
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

/// The shared `open`/`assim`/`evolve`-forecast loop: build the ensemble,
/// step it window by window, score every observation against the ensemble
/// and the deterministic nulls, optionally assimilate
/// ([`maybe_assimilate`]), and write the final [`Report`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    sc: &Scenario,
    truth: &Truth,
    cfg: &CellaConfig,
    out: &Path,
    mode: &str,
    members: usize,
    genes: Vec<GeneSpec>,
    steps_per_day: u64,
    burnt: &[CellType; 2],
    knobs: &Knobs,
    fit: Option<FitReport>,
) {
    let assim = mode == "assim";
    let rot = knobs.wind_rot_deg;
    let total = sc.grid.width * sc.grid.height;

    let ens_cfg = EnsembleConfig {
        members,
        seed: knobs.seed,
        genes: genes.clone(),
        track: vec!["Burning".into(), "BurnedOut".into()],
        beta: knobs.beta,
        sigma: knobs.sigma,
        immigrants: knobs.immigrants,
        crossover: knobs.crossover,
        immigrant_reset: knobs.immigrant_reset,
        immigrant_reset_gate: knobs.immigrant_reset_gate,
        state_correction: knobs.state_correction,
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
    let ignition: Vec<bool> = ens.member_mask(0, burnt);
    let dist = chamfer_from(&ignition, sc.grid.width, sc.grid.height);
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| (dist[i], i));
    // The Ellipse null (E41, ERA5 variant), grown window by window from its
    // own previous mask the same way the Circle above grows from the
    // ignition set — except its growth is anisotropic and wind direction
    // changes window to window, so (unlike the Circle's fixed `order`) the
    // mask genuinely has to be carried forward rather than recomputed.
    let mut ellipse_mask = ignition.clone();
    // The observed mask one window back, for the lagged nulls below;
    // starts at the ignition mask, same as `ellipse_mask`, and is only
    // ever read as "yesterday's true perimeter", never grown from itself.
    let mut prev_obs = ignition.clone();

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
            let prob = ens.state_probability(burnt);
            let mut ious = Vec::with_capacity(members);
            let mut areas = Vec::with_capacity(members);
            for i in 0..members {
                let mask = ens.member_mask(i, burnt);
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
            // Lagged nulls (controller finding on E40): the fair dummy
            // competitors for a mode that also only ever sees the mask at
            // t_{k-1}. `None` at the very first scored window, where the
            // only "previous" mask is the ignition one already reported as
            // persistence/radial.
            let is_first_obs = obs_idx == 1;
            let (lagged_persistence_iou, brier_lagged_persistence, lagged_circle_iou, brier_lagged_circle) =
                if is_first_obs {
                    (None, None, None, None)
                } else {
                    let lagged_persistence_prob: Vec<f32> =
                        prev_obs.iter().map(|&b| b as u8 as f32).collect();
                    let lagged_circle = radial_mask_from(
                        &prev_obs,
                        sc.grid.width,
                        sc.grid.height,
                        obs_n as usize,
                    );
                    let lagged_circle_prob: Vec<f32> =
                        lagged_circle.iter().map(|&b| b as u8 as f32).collect();
                    (
                        Some(iou(&prev_obs, &obs)),
                        Some(brier(&lagged_persistence_prob, &obs)),
                        Some(iou(&lagged_circle, &obs)),
                        Some(brier(&lagged_circle_prob, &obs)),
                    )
                };
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
                lagged_persistence_iou,
                brier_lagged_persistence,
                lagged_circle_iou,
                brier_lagged_circle,
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
            maybe_assimilate(
                &mut ens,
                assim,
                &obs,
                burnt,
                obs_idx,
                truth.observed_at.len(),
                knobs.assim_every,
            );
            prev_obs = obs;
            obs_idx += 1;
            if scores.len() >= knobs.max_days {
                break;
            }
        }
    }

    let n = scores.len() as f64;
    let mean = |f: &dyn Fn(&ObsScore) -> f64| scores.iter().map(f).sum::<f64>() / n;
    // Lagged-null means, over the windows that have one (k >= 1); 0.0 if
    // the series never had a second observation.
    let mean_lagged = |f: &dyn Fn(&ObsScore) -> Option<f64>| -> f64 {
        let vals: Vec<f64> = scores.iter().filter_map(f).collect();
        if vals.is_empty() {
            0.0
        } else {
            vals.iter().sum::<f64>() / vals.len() as f64
        }
    };
    let (binary_git, binary_built_utc) = provenance();
    let report = Report {
        scenario: sc.id.clone(),
        mode: mode.to_string(),
        binary_git,
        binary_built_utc,
        members,
        beta: ens_cfg.beta,
        sigma: ens_cfg.sigma,
        immigrants: ens_cfg.immigrants,
        assim_every: knobs.assim_every,
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
        mean_lagged_persistence_iou: mean_lagged(&|s| s.lagged_persistence_iou),
        mean_brier_lagged_persistence: mean_lagged(&|s| s.brier_lagged_persistence),
        mean_lagged_circle_iou: mean_lagged(&|s| s.lagged_circle_iou),
        mean_brier_lagged_circle: mean_lagged(&|s| s.brier_lagged_circle),
        final_genomes: ens.genomes(),
        scores,
        fit,
    };
    write_json(out, &report);
    eprintln!(
        "mean consensus IoU {:.3} | best-threshold {:.3} | radial {:.3} | lagged persistence {:.3} lagged circle {:.3} | Brier ens {:.4} vs radial {:.4} -> {}",
        report.mean_consensus_iou,
        report.mean_best_threshold_iou,
        report.mean_radial_iou,
        report.mean_lagged_persistence_iou,
        report.mean_lagged_circle_iou,
        report.mean_brier_ensemble,
        report.mean_brier_radial,
        out.display()
    );
}
