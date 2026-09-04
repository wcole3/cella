//! Ensembles of wildfire runs: Monte Carlo burn probability and a
//! generational, data-assimilating filter.
//!
//! One simulation is one dice roll with one parameter guess. An ensemble is
//! `M` of them at once, each with its own parameters drawn from a *prior*
//! (a range you believe in) and its own random seed. Two things fall out:
//!
//! - **Burn probability.** The fraction of members in which a cell has
//!   burned is a per-cell probability — the product operational simulators
//!   ship (FSim, PROPAGATOR) and far better calibrated than any single map.
//! - **Learning as it burns.** When an observed perimeter arrives, score each
//!   member against it, keep the good ones (resample), nudge their parameters
//!   (mutate), let a few fresh draws in (immigrants), and keep simulating from
//!   each member's own state. This is a particle filter with the operators of
//!   a genetic algorithm. Every forecast is made *before* the observation it
//!   is later scored on, so the skill is honest.
//!
//! Configure it in the JSON config next to the model:
//!
//! ```json
//! "ensemble": {
//!   "members": 32,
//!   "seed": 0,
//!   "prior": { "p0": [0.08, 0.6], "burn_duration": [5, 20],
//!              "tau_days": [2.0, 100.0], "wind_scale": [0.0, 1.5] },
//!   "beta": 10.0, "sigma": 0.2, "immigrants": 0.2
//! }
//! ```
//!
//! then [`crate::config::CellaConfig::build_ensemble`]. See `docs/ensemble.md`
//! for a walk-through and the validation experiments E24/E25 for what it buys.
//!
//! Memory: members share the model's static slope table (an `Arc`), so a
//! member costs roughly one cell-type array plus one `p_base` array.

use serde::{Deserialize, Serialize};

use crate::config::CellaConfig;
use crate::external::ModelError;
use crate::types::CellType;
use crate::wildfire::WildfireModel;
use crate::Grid2D;

/// Ranges the members' parameters are drawn from. `p0` and `tau_days` are
/// log-uniform (they span decades); the others uniform.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WildfirePrior {
    /// Base ignition probability range.
    pub p0: [f64; 2],
    /// Burn duration range, ticks (inclusive).
    pub burn_duration: [u32; 2],
    /// Containment time-scale range, days. Values ≥ 150 mean "no decay".
    pub tau_days: [f64; 2],
    /// Multiplier on the scheduled wind speed.
    pub wind_scale: [f64; 2],
    /// Optional containment-probability operator (see [`ContainmentPrior`]).
    /// `None` = off: fires stop only through the decay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<ContainmentPrior>,
}

impl Default for WildfirePrior {
    fn default() -> Self {
        WildfirePrior {
            p0: [0.08, 0.6],
            burn_duration: [5, 20],
            tau_days: [2.0, 100.0],
            wind_scale: [0.0, 1.5],
            containment: None,
        }
    }
}

/// Prior for the containment-probability operator: the way FSim (Finney et
/// al. 2011) and the 2025 "generalized containment algorithm" stop simulated
/// fires. Once a day each member is *contained* — its p0 drops to zero and
/// it stays as it is — with probability
///
/// ```text
/// P = 1 / (1 + exp(−(a + b · ln g)))      g = cells burned today / cells burned before today
/// ```
///
/// so a fire that grew 2 % yesterday is far more likely to be caught than one
/// that grew 50 % (`b` is negative). `a` sets the overall level. Both are
/// member parameters drawn from these ranges and learned by the filter like
/// the rest, so the observed ICS-209 containment record can *check* the
/// learned rate instead of fitting it. Call [`WildfireEnsemble::end_of_day`]
/// once per simulated day to apply it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContainmentPrior {
    /// Intercept range (log-odds of daily containment at g = 1).
    pub a: [f64; 2],
    /// Slope on ln(g) range; negative = slow growth gets contained.
    pub b: [f64; 2],
}

impl Default for ContainmentPrior {
    fn default() -> Self {
        ContainmentPrior { a: [-6.0, -1.0], b: [-2.0, -0.3] }
    }
}

/// Ensemble settings. Everything has a default that matches validation
/// experiment E25's recommended configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EnsembleConfig {
    /// Number of members. 32 is enough for a smooth probability map; the
    /// cost is linear in members.
    #[serde(default = "default_members")]
    pub members: usize,
    /// Master seed: fixes the prior draw and every member's RNG, so a run is
    /// reproducible.
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub prior: WildfirePrior,
    /// Selection sharpness when assimilating: weights are `exp(beta * IoU)`.
    /// 10 keeps ~25 of 32 members alive per generation; 30 is greedy.
    #[serde(default = "default_beta")]
    pub beta: f64,
    /// Mutation size: log-normal jitter on `p0` and `tau_days`, ±2 ticks on
    /// burn duration, ±0.5·sigma on the wind multiplier.
    #[serde(default = "default_sigma")]
    pub sigma: f64,
    /// Fraction of each new generation re-drawn from the prior (with a
    /// resampled parent's fire state). Keeps diversity; 0.2 recommended.
    #[serde(default = "default_immigrants")]
    pub immigrants: f64,
}

fn default_members() -> usize {
    32
}
fn default_beta() -> f64 {
    10.0
}
fn default_sigma() -> f64 {
    0.2
}
fn default_immigrants() -> f64 {
    0.2
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        EnsembleConfig {
            members: default_members(),
            seed: 0,
            prior: WildfirePrior::default(),
            beta: default_beta(),
            sigma: default_sigma(),
            immigrants: default_immigrants(),
        }
    }
}

/// One member's sampled parameters.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct MemberParams {
    pub p0: f64,
    pub burn_duration: u32,
    pub tau_days: f64,
    pub wind_scale: f64,
    /// Containment intercept (only meaningful when the prior enables it).
    #[serde(default)]
    pub contain_a: f64,
    /// Containment slope on ln(growth).
    #[serde(default)]
    pub contain_b: f64,
}

/// What [`WildfireEnsemble::assimilate`] reports about one generation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AssimilationReport {
    /// IoU of each member's burned set against the observation, before
    /// resampling.
    pub member_iou: Vec<f64>,
    /// Effective sample size of the weights (members ≈ flat, 1 ≈ collapsed).
    pub effective_sample_size: f64,
    /// Parent index chosen for each new member.
    pub parents: Vec<usize>,
    /// Members that received fresh prior parameters.
    pub immigrants: usize,
}

/// SplitMix64 — small, fast, reproducible; no extra crate.
#[derive(Clone, Debug)]
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

struct Member {
    grid: Grid2D,
    params: MemberParams,
    seed: u64,
    /// Set by the containment operator; a contained member's p0 stays 0.
    contained: bool,
    /// Burned cells at the last `end_of_day`, for the growth rate.
    burned_at_day_start: usize,
}

/// An ensemble of wildfire grids sharing one landscape.
pub struct WildfireEnsemble {
    config: EnsembleConfig,
    members: Vec<Member>,
    rng: Rng,
    next_seed: u64,
    generation: usize,
    burning: CellType,
    burned: CellType,
    total: usize,
}

impl WildfireEnsemble {
    /// Build from a config whose `model` is a wildfire model. The grid is
    /// built and attached once and cloned per member.
    pub fn from_config(cfg: &CellaConfig, ens: &EnsembleConfig) -> Result<Self, ModelError> {
        let template = cfg
            .build_grid2d()
            .ok_or_else(|| ModelError::InvalidParam("config does not build a 2D grid".into()))?;
        Self::from_template(template, ens)
    }

    /// Build from an already attached grid (must carry a [`WildfireModel`]).
    pub fn from_template(mut template: Grid2D, ens: &EnsembleConfig) -> Result<Self, ModelError> {
        if ens.members == 0 {
            return Err(ModelError::InvalidParam("ensemble needs at least one member".into()));
        }
        if ens.sigma < 0.0 || !(0.0..=1.0).contains(&ens.immigrants) {
            return Err(ModelError::InvalidParam(
                "ensemble: sigma must be >= 0 and immigrants in [0, 1]".into(),
            ));
        }
        let pr = &ens.prior;
        if pr.p0[0] <= 0.0 || pr.p0[1] > 1.0 || pr.p0[0] > pr.p0[1] {
            return Err(ModelError::InvalidParam("ensemble prior: p0 range must sit in (0, 1]".into()));
        }
        if pr.burn_duration[0] == 0 || pr.burn_duration[0] > pr.burn_duration[1] {
            return Err(ModelError::InvalidParam("ensemble prior: burn_duration range must be >= 1".into()));
        }
        if pr.tau_days[0] <= 0.0 || pr.tau_days[0] > pr.tau_days[1] {
            return Err(ModelError::InvalidParam("ensemble prior: tau_days range must be positive".into()));
        }
        if pr.wind_scale[0] < 0.0 || pr.wind_scale[0] > pr.wind_scale[1] {
            return Err(ModelError::InvalidParam("ensemble prior: wind_scale range must be >= 0".into()));
        }
        if pr.containment.as_ref().is_some_and(|c| c.a[0] > c.a[1] || c.b[0] > c.b[1]) {
            return Err(ModelError::InvalidParam("ensemble prior: containment ranges must be ordered".into()));
        }
        // Make sure the template really carries a wildfire model.
        template
            .model_mut()
            .and_then(|m| m.as_any_mut().downcast_mut::<WildfireModel>())
            .ok_or_else(|| ModelError::InvalidParam("ensemble needs a WildfireModel".into()))?;
        let total = template.width * template.height;
        let mut rng = Rng(0xC0FF_EE00 ^ ens.seed.wrapping_mul(0x9E37_79B9));
        let base = ens.seed.wrapping_mul(1_000_000);
        let mut members = Vec::with_capacity(ens.members);
        for i in 0..ens.members {
            let params = sample_prior(&mut rng, pr);
            members.push(Member {
                grid: template.clone(),
                params,
                seed: base + i as u64,
                contained: false,
                burned_at_day_start: 0,
            });
        }
        Ok(WildfireEnsemble {
            config: ens.clone(),
            members,
            rng,
            next_seed: base + ens.members as u64,
            generation: 0,
            burning: CellType::new("Burning"),
            burned: CellType::new("BurnedOut"),
            total,
        })
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Generations completed (assimilation steps so far).
    pub fn generation(&self) -> usize {
        self.generation
    }

    pub fn config(&self) -> &EnsembleConfig {
        &self.config
    }

    /// Each member's current parameters.
    pub fn params(&self) -> Vec<MemberParams> {
        self.members.iter().map(|m| m.params).collect()
    }

    /// Which members the containment operator has stopped.
    pub fn contained(&self) -> Vec<bool> {
        self.members.iter().map(|m| m.contained).collect()
    }

    /// Fraction of members currently contained.
    pub fn contained_fraction(&self) -> f64 {
        self.members.iter().filter(|m| m.contained).count() as f64 / self.members.len().max(1) as f64
    }

    /// Read access to member grids (for painting, scoring, snapshots).
    pub fn grids(&self) -> impl Iterator<Item = &Grid2D> {
        self.members.iter().map(|m| &m.grid)
    }

    /// Mutable access to member grids, e.g. to paint a fire line into every
    /// member between steps.
    pub fn grids_mut(&mut self) -> impl Iterator<Item = &mut Grid2D> {
        self.members.iter_mut().map(|m| &mut m.grid)
    }

    /// Apply this window's weather to every member: wind (scaled by the
    /// member's multiplier) and p0 (the member's own p0 times its containment
    /// decay at `hours` since ignition). Call before stepping each window.
    pub fn set_weather(&mut self, hours: f64, wind_speed_ms: f64, wind_from_deg: f64) {
        for m in self.members.iter_mut() {
            let model = wildfire_of(&mut m.grid);
            model.params.seed = m.seed;
            model.params.burn_duration = m.params.burn_duration;
            model.params.wind_speed = wind_speed_ms * m.params.wind_scale;
            model.params.wind_from_deg = wind_from_deg.rem_euclid(360.0);
            let decay = if m.params.tau_days >= 150.0 {
                1.0
            } else {
                (-hours / (24.0 * m.params.tau_days)).exp()
            };
            let p0 = if m.contained { 0.0 } else { (m.params.p0 * decay).min(1.0) };
            model.set_p0(p0).expect("prior keeps p0 in range");
        }
    }

    /// Apply the containment operator once per simulated day (no-op when the
    /// prior has no `containment`). Each still-burning member computes its
    /// growth since the previous call and is contained with the logistic
    /// probability described on [`ContainmentPrior`]. Returns how many
    /// members were contained by this call.
    pub fn end_of_day(&mut self) -> usize {
        let Some(_) = self.config.prior.containment else {
            for m in self.members.iter_mut() {
                m.burned_at_day_start = 0;
            }
            return 0;
        };
        let mut newly = 0;
        for i in 0..self.members.len() {
            let burned = self.member_burned(i).iter().filter(|&&b| b).count();
            let m = &mut self.members[i];
            let before = m.burned_at_day_start.max(1);
            if m.burned_at_day_start > 0 && !m.contained {
                let g = ((burned.saturating_sub(m.burned_at_day_start)) as f64 / before as f64).max(1e-4);
                let logit = m.params.contain_a + m.params.contain_b * g.ln();
                let p = 1.0 / (1.0 + (-logit).exp());
                if self.rng.uniform() < p {
                    m.contained = true;
                    newly += 1;
                    let model = wildfire_of(&mut m.grid);
                    model.set_p0(0.0).expect("zero is in range");
                }
            }
            m.burned_at_day_start = burned;
        }
        newly
    }

    /// Advance every member one tick.
    pub fn step(&mut self) {
        for m in self.members.iter_mut() {
            m.grid.step();
        }
    }

    /// Advance every member `n` ticks.
    pub fn step_n(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// One member's burned set (Burning or BurnedOut).
    pub fn member_burned(&self, i: usize) -> Vec<bool> {
        let g = &self.members[i].grid;
        (0..self.total)
            .map(|c| {
                let t = g.cell_type(c);
                t == self.burning || t == self.burned
            })
            .collect()
    }

    /// Per-cell burn probability: fraction of members in which the cell has
    /// burned.
    pub fn burn_probability(&self) -> Vec<f32> {
        let mut p = vec![0.0f32; self.total];
        for m in &self.members {
            for (c, slot) in p.iter_mut().enumerate() {
                let t = m.grid.cell_type(c);
                if t == self.burning || t == self.burned {
                    *slot += 1.0;
                }
            }
        }
        let inv = 1.0 / self.members.len() as f32;
        for slot in p.iter_mut() {
            *slot *= inv;
        }
        p
    }

    /// Cells whose burn probability is at least `threshold` (0.5 = majority).
    pub fn consensus(&self, threshold: f32) -> Vec<bool> {
        self.burn_probability().iter().map(|&p| p >= threshold).collect()
    }

    /// One generation of learning from an observed burned mask: score, weight
    /// by `exp(beta * IoU)`, resample, mutate, admit immigrants. Members keep
    /// their (parent's) fire state; only parameters and seeds change. Call
    /// this *after* you have scored the forecast against `observed`, never
    /// before.
    pub fn assimilate(&mut self, observed: &[bool]) -> Result<AssimilationReport, ModelError> {
        if observed.len() != self.total {
            return Err(ModelError::LayerLength {
                layer: "observed",
                expected: self.total,
                got: observed.len(),
            });
        }
        let m = self.members.len();
        let ious: Vec<f64> = (0..m).map(|i| iou(&self.member_burned(i), observed)).collect();
        let max = ious.iter().cloned().fold(f64::MIN, f64::max);
        let w: Vec<f64> = ious.iter().map(|&v| (self.config.beta * (v - max)).exp()).collect();
        let wsum: f64 = w.iter().sum();
        let ess = wsum * wsum / w.iter().map(|x| x * x).sum::<f64>();
        // Systematic resampling.
        let mut parents = Vec::with_capacity(m);
        let u0 = self.rng.uniform() / m as f64;
        let (mut cum, mut j) = (w[0] / wsum, 0usize);
        for i in 0..m {
            let u = u0 + i as f64 / m as f64;
            while u > cum && j + 1 < m {
                j += 1;
                cum += w[j] / wsum;
            }
            parents.push(j);
        }
        let n_imm = (self.config.immigrants * m as f64).round() as usize;
        let mut children: Vec<Member> = Vec::with_capacity(m);
        for (ci, &pi) in parents.iter().enumerate() {
            self.next_seed += 1;
            let params = if ci < n_imm {
                sample_prior(&mut self.rng, &self.config.prior)
            } else {
                mutate(self.members[pi].params, &mut self.rng, self.config.sigma, &self.config.prior)
            };
            children.push(Member {
                grid: self.members[pi].grid.clone(),
                params,
                seed: self.next_seed,
                contained: self.members[pi].contained,
                burned_at_day_start: self.members[pi].burned_at_day_start,
            });
        }
        self.members = children;
        self.generation += 1;
        Ok(AssimilationReport {
            member_iou: ious,
            effective_sample_size: ess,
            parents,
            immigrants: n_imm,
        })
    }
}

fn wildfire_of(g: &mut Grid2D) -> &mut WildfireModel {
    g.model_mut()
        .expect("ensemble members carry a model")
        .as_any_mut()
        .downcast_mut::<WildfireModel>()
        .expect("ensemble members carry a WildfireModel")
}

fn sample_prior(rng: &mut Rng, pr: &WildfirePrior) -> MemberParams {
    let span = (pr.burn_duration[1] - pr.burn_duration[0] + 1) as f64;
    let (a, b) = match &pr.containment {
        Some(c) => (
            c.a[0] + rng.uniform() * (c.a[1] - c.a[0]),
            c.b[0] + rng.uniform() * (c.b[1] - c.b[0]),
        ),
        None => (0.0, 0.0),
    };
    MemberParams {
        p0: rng.log_uniform(pr.p0[0], pr.p0[1]),
        burn_duration: pr.burn_duration[0] + (rng.uniform() * span).floor().min(span - 1.0) as u32,
        tau_days: rng.log_uniform(pr.tau_days[0], pr.tau_days[1]),
        wind_scale: pr.wind_scale[0] + rng.uniform() * (pr.wind_scale[1] - pr.wind_scale[0]),
        contain_a: a,
        contain_b: b,
    }
}

fn mutate(p: MemberParams, rng: &mut Rng, sigma: f64, pr: &WildfirePrior) -> MemberParams {
    let clampf = |v: f64, r: [f64; 2]| v.clamp(r[0], r[1]);
    let dur_step = (rng.uniform() * 5.0).floor() as i64 - 2; // -2..=2
    let (a, b) = match &pr.containment {
        Some(c) => (
            clampf(p.contain_a + sigma * (c.a[1] - c.a[0]) * rng.normal(), c.a),
            clampf(p.contain_b + sigma * (c.b[1] - c.b[0]) * rng.normal(), c.b),
        ),
        None => (p.contain_a, p.contain_b),
    };
    MemberParams {
        p0: clampf(p.p0 * (sigma * rng.normal()).exp(), pr.p0),
        burn_duration: (p.burn_duration as i64 + dur_step)
            .clamp(pr.burn_duration[0] as i64, pr.burn_duration[1] as i64) as u32,
        tau_days: clampf(p.tau_days * (sigma * rng.normal()).exp(), pr.tau_days),
        wind_scale: clampf(p.wind_scale + 0.5 * sigma * rng.normal(), pr.wind_scale),
        contain_a: a,
        contain_b: b,
    }
}

/// Intersection over union of two masks (1.0 when both are empty).
pub fn iou(a: &[bool], b: &[bool]) -> f64 {
    let (mut inter, mut union) = (0u64, 0u64);
    for (&x, &y) in a.iter().zip(b) {
        inter += (x && y) as u64;
        union += (x || y) as u64;
    }
    if union == 0 { 1.0 } else { inter as f64 / union as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wildfire::{FuelClass, WildfireEnv, WildfireParams};
    use crate::Rule2D;

    fn template(w: usize, h: usize) -> Grid2D {
        let forest = CellType::new("Forest");
        let mut cells = vec![forest; w * h];
        cells[(h / 2) * w + w / 2] = CellType::new("Burning");
        let params = WildfireParams {
            seed: 0,
            p0: 0.3,
            fuels: vec![FuelClass { name: "Forest".into(), veg_factor: 1.0 }],
            wind_speed: 0.0,
            wind_from_deg: 270.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 5,
            spotting: None,
            burning_name: None,
            burned_name: None,
        };
        let mut g = Grid2D::new(w, h, 0, cells, Rule2D { subrules: vec![] });
        g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
            .unwrap();
        g
    }

    fn small_config() -> EnsembleConfig {
        EnsembleConfig { members: 8, ..EnsembleConfig::default() }
    }

    #[test]
    fn members_draw_inside_the_prior_and_are_reproducible() {
        let e1 = WildfireEnsemble::from_template(template(12, 12), &small_config()).unwrap();
        let e2 = WildfireEnsemble::from_template(template(12, 12), &small_config()).unwrap();
        assert_eq!(e1.params(), e2.params(), "same seed, same draw");
        let pr = WildfirePrior::default();
        for p in e1.params() {
            assert!(p.p0 >= pr.p0[0] && p.p0 <= pr.p0[1]);
            assert!(p.burn_duration >= pr.burn_duration[0] && p.burn_duration <= pr.burn_duration[1]);
            assert!(p.tau_days >= pr.tau_days[0] && p.tau_days <= pr.tau_days[1]);
            assert!(p.wind_scale >= pr.wind_scale[0] && p.wind_scale <= pr.wind_scale[1]);
        }
        let e3 = WildfireEnsemble::from_template(
            template(12, 12),
            &EnsembleConfig { seed: 7, ..small_config() },
        )
        .unwrap();
        assert_ne!(e1.params(), e3.params(), "a different seed draws differently");
    }

    #[test]
    fn burn_probability_is_a_fraction_of_members_and_grows_with_steps() {
        let mut e = WildfireEnsemble::from_template(template(16, 16), &small_config()).unwrap();
        let p0 = e.burn_probability();
        assert!(p0.iter().all(|&p| (0.0..=1.0).contains(&p)));
        assert_eq!(p0.iter().filter(|&&p| p == 1.0).count(), 1, "only the ignition, in every member");
        e.set_weather(0.0, 0.0, 270.0);
        e.step_n(6);
        let p1 = e.burn_probability();
        assert!(p1.iter().sum::<f32>() > p0.iter().sum::<f32>(), "fire spread in some members");
        assert!(
            p1.iter().any(|&p| p > 0.0 && p < 1.0),
            "members with different p0 disagree somewhere"
        );
        let cons = e.consensus(0.5);
        assert_eq!(cons.len(), 256);
        assert!(cons[8 * 16 + 8], "the ignition is in every member's burned set");
    }

    #[test]
    fn assimilation_resamples_toward_members_that_match_and_admits_immigrants() {
        let cfg = EnsembleConfig { members: 8, beta: 30.0, immigrants: 0.25, ..EnsembleConfig::default() };
        let mut e = WildfireEnsemble::from_template(template(16, 16), &cfg).unwrap();
        e.set_weather(0.0, 0.0, 270.0);
        e.step_n(8);
        // Pretend the observation is exactly member 0's state.
        let observed = e.member_burned(0);
        let before = e.params();
        let rep = e.assimilate(&observed).unwrap();
        assert_eq!(rep.member_iou.len(), 8);
        assert_eq!(rep.member_iou[0], 1.0);
        assert_eq!(rep.immigrants, 2);
        assert!(rep.effective_sample_size < 8.0, "weights are not flat");
        assert!(rep.parents.iter().filter(|&&p| p == 0).count() >= 4, "member 0 dominates: {:?}", rep.parents);
        assert_eq!(e.generation(), 1);
        // Children of member 0 that were not immigrants sit near its parameters.
        let after = e.params();
        let near: usize = rep
            .parents
            .iter()
            .zip(after.iter())
            .skip(rep.immigrants)
            .filter(|(p, c)| **p == 0 && (c.p0 / before[0].p0).ln().abs() < 1.0)
            .count();
        assert!(near >= 3, "mutation is a nudge, not a re-draw");
        // The fire state carried over: the consensus still contains the ignition.
        assert!(e.consensus(0.5)[8 * 16 + 8]);
        // Wrong observation length is refused.
        assert!(e.assimilate(&observed[..10]).is_err());
    }

    #[test]
    fn containment_operator_stops_slow_members_and_keeps_them_stopped() {
        // A prior that makes containment near-certain for any growth.
        let prior = WildfirePrior {
            containment: Some(ContainmentPrior { a: [8.0, 8.0], b: [0.0, 0.0] }),
            ..WildfirePrior::default()
        };
        let cfg = EnsembleConfig { members: 6, prior, ..EnsembleConfig::default() };
        let mut e = WildfireEnsemble::from_template(template(16, 16), &cfg).unwrap();
        e.set_weather(0.0, 0.0, 270.0);
        assert_eq!(e.end_of_day(), 0, "the first call only records the starting size");
        e.step_n(6);
        let newly = e.end_of_day();
        assert!(newly >= 5, "near-certain containment: {newly} of 6");
        let frac = e.contained_fraction();
        assert!(frac >= 5.0 / 6.0);
        // Contained members no longer spread, even after the weather is re-applied.
        let before = e.burn_probability();
        e.set_weather(24.0, 0.0, 270.0);
        e.step_n(6);
        let after = e.burn_probability();
        let grew: usize = after.iter().zip(&before).filter(|(a, b)| a > b).count();
        assert!(grew <= 16 * 16 / 4, "only the uncontained minority can add cells: {grew}");
        // Containment survives resampling.
        let obs = e.member_burned(0);
        e.assimilate(&obs).unwrap();
        assert!(e.contained_fraction() >= 5.0 / 6.0 - 1e-9 || e.contained_fraction() > 0.5);
        // Without a containment prior end_of_day is a no-op.
        let mut plain = WildfireEnsemble::from_template(template(8, 8), &small_config()).unwrap();
        plain.set_weather(0.0, 0.0, 270.0);
        plain.end_of_day();
        plain.step_n(3);
        assert_eq!(plain.end_of_day(), 0);
        assert_eq!(plain.contained_fraction(), 0.0);
    }

    #[test]
    fn config_is_validated() {
        assert!(WildfireEnsemble::from_template(template(4, 4), &EnsembleConfig { members: 0, ..Default::default() }).is_err());
        let bad = EnsembleConfig {
            prior: WildfirePrior { p0: [0.0, 0.5], ..WildfirePrior::default() },
            ..EnsembleConfig::default()
        };
        assert!(WildfireEnsemble::from_template(template(4, 4), &bad).is_err());
        let bad = EnsembleConfig { immigrants: 1.5, ..EnsembleConfig::default() };
        assert!(WildfireEnsemble::from_template(template(4, 4), &bad).is_err());
        // A grid without a wildfire model is refused.
        let g = Grid2D::new(4, 4, 0, vec![CellType::new("A"); 16], Rule2D { subrules: vec![] });
        assert!(WildfireEnsemble::from_template(g, &EnsembleConfig::default()).is_err());
    }

    #[test]
    fn config_json_round_trips_with_defaults() {
        let c: EnsembleConfig = serde_json::from_str(r#"{"members": 4}"#).unwrap();
        assert_eq!(c.members, 4);
        assert_eq!(c.beta, 10.0);
        assert_eq!(c.prior, WildfirePrior::default());
        let s = serde_json::to_string(&c).unwrap();
        let back: EnsembleConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
    }
}
