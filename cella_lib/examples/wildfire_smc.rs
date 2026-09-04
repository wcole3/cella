//! Ensemble forecasting for the wildfire model: Monte Carlo burn probability
//! and a generational, data-assimilating ensemble (validation E24 / E25).
//!
//! `open` mode — plain Monte Carlo. `M` members are drawn from a broad prior
//! over (p0, burn_duration, containment tau, wind multiplier) and run
//! independently. At every observation time the ensemble is summarised as a
//! per-cell burn probability, scored as a probabilistic forecast (Brier
//! score, best-threshold IoU, consensus IoU) next to the deterministic
//! nulls (persistence, area-matched radial).
//!
//! `assim` mode — a particle filter with parameter evolution. Same prior and
//! the same members, but at each observation time every member's *current*
//! burned set is compared with the observed mask, members are resampled with
//! weights exp(beta * IoU), and the children inherit the parent's grid state
//! with mutated parameters (log-normal jitter) and a fresh RNG seed. Then the
//! ensemble keeps simulating from where it is. The scores recorded at
//! observation time t_k are therefore one-window-ahead FORECASTS made from
//! the state assimilated at t_{k-1} — no member ever sees the mask it is
//! scored against before it is scored. This is the honest use of truth: the
//! way an operational nowcast would use yesterday's perimeter.
//!
//! Usage (from cella_lib/):
//!   cargo run --release --example wildfire_smc -- <scenario_dir> <members> <open|assim> <out.json>
//! Env: SMC_BETA (default 10), SMC_SIGMA (mutation, default 0.2), SMC_SEED (0),
//!      SMC_WIND_ROT_DEG (0), SMC_PRIOR=path.json (override prior ranges),
//!      SMC_ASSIM_EVERY=k (resample only every k-th observation, so the
//!      scores in between are k-window-ahead forecasts; default 1),
//!      SMC_IMMIGRANTS (fraction of each generation re-drawn from the prior,
//!      default 0: keeps diversity so the filter cannot collapse onto one
//!      parameter set — the GA "immigration" operator).

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
    from_deg: f64,
}

#[derive(Deserialize)]
struct Truth {
    format_version: u32,
    observed_at: Vec<f64>,
    arrival_hours: Vec<f64>,
}

/// Prior ranges; log-uniform for p0 and tau, uniform otherwise.
#[derive(Clone, Deserialize, Serialize)]
struct Prior {
    p0: [f64; 2],
    dur: [u32; 2],
    tau_days: [f64; 2],
    wind_scale: [f64; 2],
}

impl Default for Prior {
    fn default() -> Self {
        Prior {
            p0: [0.08, 0.6],
            dur: [5, 20],
            tau_days: [2.0, 100.0],
            wind_scale: [0.0, 1.5],
        }
    }
}

#[derive(Clone, Copy, Serialize)]
struct Params {
    p0: f64,
    dur: u32,
    tau_days: f64,
    wind_scale: f64,
}

struct Member {
    grid: Grid2D,
    params: Params,
    seed: u64,
}

/// SplitMix64: enough randomness for sampling a prior, no extra crate.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let (u, v) = (self.uniform().max(1e-12), self.uniform());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }
    fn log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        (lo.ln() + self.uniform() * (hi.ln() - lo.ln())).exp()
    }
}

#[derive(Serialize)]
struct ObsScore {
    hours: f64,
    obs_burned: u64,
    /// Mean over members of IoU(member burned set, observed).
    mean_member_iou: f64,
    best_member_iou: f64,
    /// IoU of the cells with burn probability >= 0.5.
    consensus_iou: f64,
    /// IoU of the union of members (probability > 0).
    union_iou: f64,
    /// Best IoU over thresholds 0.1..=0.9 and the threshold that gave it.
    best_threshold_iou: f64,
    best_threshold: f64,
    /// Mean burned cells across members / observed.
    area_ratio_mean: f64,
    /// Brier score of the ensemble probability map (lower is better) and of
    /// the deterministic nulls on the same truth.
    brier_ensemble: f64,
    brier_radial: f64,
    brier_persistence: f64,
    radial_iou: f64,
    persistence_iou: f64,
    /// Effective sample size before resampling (assim mode) or M.
    ess: f64,
    /// Ensemble parameter statistics (mean, std) after this step.
    p0_mean: f64,
    p0_std: f64,
    tau_mean: f64,
    tau_std: f64,
    dur_mean: f64,
    wind_scale_mean: f64,
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
    prior: Prior,
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
    final_params: Vec<Params>,
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
    let (mut inter, mut union) = (0u64, 0u64);
    for (&x, &y) in a.iter().zip(b) {
        inter += (x && y) as u64;
        union += (x || y) as u64;
    }
    if union == 0 { 1.0 } else { inter as f64 / union as f64 }
}

fn brier(prob: &[f64], obs: &[bool]) -> f64 {
    prob.iter()
        .zip(obs)
        .map(|(&p, &o)| (p - if o { 1.0 } else { 0.0 }).powi(2))
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

fn burned_mask(g: &Grid2D, burning: CellType, burned: CellType, total: usize) -> Vec<bool> {
    (0..total)
        .map(|i| {
            let t = g.cell_type(i);
            t == burning || t == burned
        })
        .collect()
}

fn set_member_window(m: &mut Member, wind: &WindEntry, rot: f64) {
    let model = m
        .grid
        .model_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<WildfireModel>()
        .unwrap();
    model.params.seed = m.seed;
    model.params.burn_duration = m.params.dur;
    model.params.wind_speed = wind.speed_ms * m.params.wind_scale;
    model.params.wind_from_deg = (wind.from_deg + rot).rem_euclid(360.0);
    let decay = if m.params.tau_days >= 150.0 {
        1.0
    } else {
        (-wind.hours / (24.0 * m.params.tau_days)).exp()
    };
    model.set_p0((m.params.p0 * decay).min(1.0)).expect("p0 in range");
}

fn sample_prior(rng: &mut Rng, prior: &Prior) -> Params {
    let dur_span = (prior.dur[1] - prior.dur[0] + 1) as f64;
    Params {
        p0: rng.log_uniform(prior.p0[0], prior.p0[1]),
        dur: prior.dur[0] + (rng.uniform() * dur_span).floor().min(dur_span - 1.0) as u32,
        tau_days: rng.log_uniform(prior.tau_days[0], prior.tau_days[1]),
        wind_scale: prior.wind_scale[0] + rng.uniform() * (prior.wind_scale[1] - prior.wind_scale[0]),
    }
}

fn mutate(p: Params, rng: &mut Rng, sigma: f64, prior: &Prior) -> Params {
    let clampf = |v: f64, r: [f64; 2]| v.clamp(r[0], r[1]);
    let dur_step = (rng.uniform() * 5.0).floor() as i64 - 2; // -2..=2
    Params {
        p0: clampf(p.p0 * (sigma * rng.normal()).exp(), prior.p0),
        dur: (p.dur as i64 + dur_step).clamp(prior.dur[0] as i64, prior.dur[1] as i64) as u32,
        tau_days: clampf(p.tau_days * (sigma * rng.normal()).exp(), prior.tau_days),
        wind_scale: clampf(p.wind_scale + 0.5 * sigma * rng.normal(), prior.wind_scale),
    }
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
    let beta = envf("SMC_BETA", 10.0);
    let sigma = envf("SMC_SIGMA", 0.2);
    let base_seed = envf("SMC_SEED", 0.0) as u64;
    let rot = envf("SMC_WIND_ROT_DEG", 0.0);
    let immigrants = envf("SMC_IMMIGRANTS", 0.0);
    let assim_every = envf("SMC_ASSIM_EVERY", 1.0).max(1.0) as usize;
    let prior: Prior = std::env::var("SMC_PRIOR")
        .ok()
        .map(|p| load(Path::new(&p)))
        .unwrap_or_default();
    let assim = mode == "assim";

    let sc: Scenario = load(&dir.join("scenario.json"));
    let truth: Truth = load(&dir.join("truth.json"));
    let cfg: CellaConfig = load(&dir.join("config.json"));
    assert_eq!(sc.format_version, 2, "unknown scenario format");
    assert_eq!(truth.format_version, 2, "unknown truth format");
    let total = sc.grid.width * sc.grid.height;
    let burning = CellType::new("Burning");
    let burned = CellType::new("BurnedOut");

    // Members.
    let mut rng = Rng(0xC0FFEE ^ base_seed.wrapping_mul(0x9E37_79B9));
    let mut ens: Vec<Member> = (0..members)
        .map(|i| {
            let grid: Grid2D = cfg.build_grid2d().expect("config builds");
            Member {
                grid,
                params: sample_prior(&mut rng, &prior),
                seed: base_seed * 100_000 + i as u64,
            }
        })
        .collect();
    let mut next_seed = base_seed * 100_000 + members as u64;

    // Nulls.
    let ignition = burned_mask(&ens[0].grid, burning, burned, total);
    let dist = chamfer_from(&ignition, sc.grid.width, sc.grid.height);
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| (dist[i], i));

    eprintln!(
        "{}: {}x{}, {} members, mode {mode}, beta {beta}, sigma {sigma}",
        sc.id, sc.grid.width, sc.grid.height, members
    );

    let mut scores: Vec<ObsScore> = Vec::new();
    let mut steps_done = 0u64;
    let mut obs_idx = 1usize; // observed_at[0] is t0
    let mut member_masks: Vec<Vec<bool>> = Vec::new();
    for win in sc.wind.windows(2) {
        let (cur, next) = (&win[0], &win[1]);
        for m in ens.iter_mut() {
            set_member_window(m, cur, rot);
        }
        let target = (next.hours * sc.steps_per_hour).round() as u64;
        for _ in steps_done..target {
            for m in ens.iter_mut() {
                m.grid.step();
            }
        }
        steps_done = target;

        // Observation at this window end?
        if obs_idx < truth.observed_at.len() && (truth.observed_at[obs_idx] - next.hours).abs() < 1e-6 {
            let t = next.hours;
            let obs = mask_at(&truth.arrival_hours, t);
            let obs_n = obs.iter().filter(|&&b| b).count() as u64;
            member_masks.clear();
            let mut prob = vec![0.0f64; total];
            let mut ious = Vec::with_capacity(members);
            let mut areas = Vec::with_capacity(members);
            for m in &ens {
                let mask = burned_mask(&m.grid, burning, burned, total);
                for (p, &b) in prob.iter_mut().zip(&mask) {
                    *p += b as u8 as f64;
                }
                ious.push(iou(&mask, &obs));
                areas.push(mask.iter().filter(|&&b| b).count() as f64);
                member_masks.push(mask);
            }
            for p in prob.iter_mut() {
                *p /= members as f64;
            }
            let thr_mask = |q: f64| -> Vec<bool> { prob.iter().map(|&p| p >= q).collect() };
            let consensus_iou = iou(&thr_mask(0.5), &obs);
            let union_iou = iou(&thr_mask(1e-9), &obs);
            let (mut best_thr_iou, mut best_thr) = (0.0, 0.0);
            for k in 1..=9 {
                let q = k as f64 / 10.0;
                let v = iou(&thr_mask(q), &obs);
                if v > best_thr_iou {
                    best_thr_iou = v;
                    best_thr = q;
                }
            }
            let radial: Vec<bool> = {
                let mut m = vec![false; total];
                for &i in order.iter().take(obs_n as usize) {
                    m[i] = true;
                }
                m
            };
            let radial_prob: Vec<f64> = radial.iter().map(|&b| b as u8 as f64).collect();
            let pers_prob: Vec<f64> = ignition.iter().map(|&b| b as u8 as f64).collect();
            // Weights for resampling (assim) and ESS.
            let max_iou = ious.iter().cloned().fold(0.0, f64::max);
            let w: Vec<f64> = ious.iter().map(|&v| (beta * (v - max_iou)).exp()).collect();
            let wsum: f64 = w.iter().sum();
            let ess = wsum * wsum / w.iter().map(|x| x * x).sum::<f64>();
            let (p0m, p0s) = mean_std(&ens.iter().map(|m| m.params.p0).collect::<Vec<_>>());
            let (taum, taus) = mean_std(&ens.iter().map(|m| m.params.tau_days.min(150.0)).collect::<Vec<_>>());
            let (durm, _) = mean_std(&ens.iter().map(|m| m.params.dur as f64).collect::<Vec<_>>());
            let (wsm, _) = mean_std(&ens.iter().map(|m| m.params.wind_scale).collect::<Vec<_>>());
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
            };
            eprintln!(
                "  t={t:5.0}h obs {obs_n:7} | member IoU mean {:.3} best {:.3} | consensus {:.3} bestthr {:.3}@{:.1} | radial {:.3} | Brier ens {:.4} radial {:.4} | ESS {:.1} | p0 {:.2}±{:.2} tau {:.1}",
                score.mean_member_iou, max_iou, consensus_iou, best_thr_iou, best_thr, score.radial_iou,
                score.brier_ensemble, score.brier_radial, ess, p0m, p0s, taum
            );
            scores.push(score);

            // Generational step: resample by fitness, mutate, keep simulating.
            if assim && obs_idx + 1 < truth.observed_at.len() && obs_idx.is_multiple_of(assim_every) {
                // Systematic resampling.
                let mut parents = Vec::with_capacity(members);
                let u0 = rng.uniform() / members as f64;
                let (mut cum, mut j) = (w[0] / wsum, 0usize);
                for i in 0..members {
                    let u = u0 + i as f64 / members as f64;
                    while u > cum && j + 1 < members {
                        j += 1;
                        cum += w[j] / wsum;
                    }
                    parents.push(j);
                }
                let n_imm = (immigrants * members as f64).round() as usize;
                let mut children: Vec<Member> = Vec::with_capacity(members);
                for (ci, &pi) in parents.iter().enumerate() {
                    let parent = &ens[pi];
                    next_seed += 1;
                    // Immigrants inherit a resampled parent's fire STATE (you
                    // cannot re-draw the past) but fresh prior parameters.
                    let params = if ci < n_imm {
                        sample_prior(&mut rng, &prior)
                    } else {
                        mutate(parent.params, &mut rng, sigma, &prior)
                    };
                    children.push(Member {
                        grid: parent.grid.clone(),
                        params,
                        seed: next_seed,
                    });
                }
                ens = children;
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
        beta,
        sigma,
        immigrants,
        assim_every,
        prior: prior.clone(),
        final_consensus_iou: scores.last().map_or(0.0, |s| s.consensus_iou),
        final_best_threshold_iou: scores.last().map_or(0.0, |s| s.best_threshold_iou),
        final_radial_iou: scores.last().map_or(0.0, |s| s.radial_iou),
        mean_consensus_iou: mean(&|s| s.consensus_iou),
        mean_best_threshold_iou: mean(&|s| s.best_threshold_iou),
        mean_member_iou: mean(&|s| s.mean_member_iou),
        mean_radial_iou: mean(&|s| s.radial_iou),
        mean_brier_ensemble: mean(&|s| s.brier_ensemble),
        mean_brier_radial: mean(&|s| s.brier_radial),
        final_params: ens.iter().map(|m| m.params).collect(),
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
