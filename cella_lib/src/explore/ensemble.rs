//! [`Ensemble`]: many copies of one simulation, stepping together.
//!
//! Each **member** is a clone of the template grid with its own seed and its
//! own genome (a draw from the genes' ranges). Stepping the ensemble steps
//! every member. Two things fall out:
//!
//! - **A probability map.** [`Ensemble::state_probability`] gives, per cell,
//!   the fraction of members in which the cell is in one of the *tracked*
//!   types. "70 %" means "most futures we can imagine have this cell on".
//! - **Learning from an observation.** [`Ensemble::assimilate`] scores every
//!   member against an observed mask, keeps the ones that did well (with
//!   copies), nudges their genomes, admits a few fresh draws, and lets them
//!   all keep going from where they are. Tomorrow's map is then made by
//!   members that already resemble today. Grown-up words: a particle filter
//!   whose proposal step uses genetic-algorithm operators.
//!
//! Anything model-specific — a weather schedule, a stopping rule — lives in
//! an optional [`MemberDriver`]. Without one, an ensemble is plain Monte
//! Carlo over seeds and genes, which already works for any rule.
//!
//! ```text
//! "ensemble": {
//!   "members": 32, "seed": 0,
//!   "genes": [{"key": "rule.subrules[2].randomness", "range": [0.0, 0.3]}],
//!   "track": ["Alive"],
//!   "beta": 10.0, "sigma": 0.2, "immigrants": 0.2
//! }
//! ```
//!
//! Same seed, same members, same answer — on any thread count.

use std::collections::BTreeMap;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::driver::{Forcing, MemberDriver, MemberState};
use super::genome::{GeneSpace, GeneSpec, Genome};
use super::metrics::{iou, mean_sd};
use super::sim::Sim;
use crate::external::{ModelError, ParamValue};
use crate::rng::Rng;
use crate::threads::{chunks_for_work, member_par_override, pool, thread_count};
use crate::types::CellType;

/// Settings for an ensemble (the `"ensemble"` block of a config).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnsembleConfig {
    /// How many members run together. 32 gives a smooth probability map;
    /// cost and memory grow linearly.
    #[serde(default = "default_members")]
    pub members: usize,
    /// Fixes the genome draws and every member's own seed. Same seed, same
    /// result.
    #[serde(default)]
    pub seed: u64,
    /// Which knobs vary between members, and over what range. Empty is
    /// legal: every member is then identical except for its dice.
    #[serde(default)]
    pub genes: Vec<GeneSpec>,
    /// Cell types counted as "on" by the probability map and by
    /// [`Ensemble::assimilate`]. Empty means every declared type except the
    /// background.
    #[serde(default)]
    pub track: Vec<String>,
    /// Selection sharpness when learning: weights are `exp(beta × score)`.
    /// 10 keeps a healthy spread; 30 is greedy and over-confident.
    #[serde(default = "default_beta")]
    pub beta: f64,
    /// Mutation size after resampling (see [`super::genome`]).
    #[serde(default = "default_sigma")]
    pub sigma: f64,
    /// Share of each new generation re-drawn from the gene ranges, so the
    /// population never collapses onto one genome.
    #[serde(default = "default_immigrants")]
    pub immigrants: f64,
    /// Chance that a resampled child's genome is a cross of two parents
    /// (uniform per gene) before it is mutated. 0 (the default) keeps the
    /// classic particle filter: children inherit one parent's genome.
    #[serde(default)]
    pub crossover: f64,
    /// Whether an immigrant starts with a fresh driver state instead of
    /// its parent's. A parent's state can carry decisions that should not
    /// outlive its genome (the wildfire driver's "contained" flag), and with
    /// it inherited a population in which every member has stopped can
    /// never start again. Off by default; the driver must then be able to
    /// rebuild what it needs from the genome (the wildfire driver reads
    /// `model.p0` from the gene, so that gene must be present).
    #[serde(default)]
    pub immigrant_reset: bool,
    /// Gate on [`Self::immigrant_reset`]: when set, an immigrant is given a
    /// fresh state only if the *area ratio* at the last
    /// [`Ensemble::assimilate`] call — mean member burned area over
    /// observed burned area, computed there and cached for this — is
    /// strictly below this value. Below 1 means the population is
    /// under-predicting the observed area: every member's forecast is
    /// smaller than what actually burned, which is what a population that
    /// has stopped growing while the real fire has not looks like, exactly
    /// the situation `immigrant_reset` exists to repair. Above 1 the
    /// population is keeping up or over-predicting, and resetting would
    /// only spend probability re-igniting members where nothing more is
    /// going to burn.
    ///
    /// `None` (the default) leaves the plain `immigrant_reset` bool in
    /// charge, unmodified: this is how E38's behaviour survives as an
    /// option. Set the gate and the bare bool is ignored — the gate alone
    /// decides. Before the first call to [`Ensemble::assimilate`] (or when
    /// a caller only ever uses [`Ensemble::assimilate_scores`], which does
    /// not touch the area ratio) there is no ratio to compare, so the gate
    /// treats that as "do not reset".
    #[serde(default)]
    pub immigrant_reset_gate: Option<f64>,
    /// Which children get their *grid* rebuilt from the observation
    /// (state correction: Rochoux et al. 2014; Xue, Gu & Hu 2012) instead
    /// of inheriting a resampled parent's, via
    /// [`MemberDriver::seed_from_observation`].
    ///
    /// - `None` (default): nobody does — a child's grid is always a clone
    ///   of its resampled parent, exactly the particle filter before E40.
    ///   Only `immigrant_reset` / `immigrant_reset_gate` decide whether an
    ///   immigrant's driver *state* also carries over.
    /// - `Immigrants`: only the immigrants (E40) — the rest of the
    ///   population still inherits a parent's grid untouched.
    /// - `All`: every resampled child (E40b) — a child still keeps its own
    ///   resampled-and-mutated genome (or, for an immigrant, its fresh
    ///   draw from the prior), so learning continues exactly as before;
    ///   only the *grid* is corrected, every window, for everyone.
    ///
    /// A child whose grid is rebuilt this way (`Immigrants` for an
    /// immigrant, `All` for anyone) always gets a fresh, uncontained
    /// driver state too — it has no history to have been contained *in* —
    /// so `immigrant_reset_gate` and `immigrant_reset` are not consulted
    /// for it at all. They still govern an immigrant under `None`, and
    /// (since `All` only ever corrects immigrants the same way `Immigrants`
    /// does) are never consulted under `All` either — every child is
    /// covered by the correction directly.
    #[serde(default)]
    pub state_correction: StateCorrection,
    /// Optional model-specific behaviour (see [`MemberDriver`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<Box<dyn MemberDriver>>,
}

/// See [`EnsembleConfig::state_correction`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateCorrection {
    /// Nobody's grid is rebuilt from the observation. Default.
    #[default]
    None,
    /// Only the immigrants' grids are rebuilt from the observation (E40).
    Immigrants,
    /// Every resampled child's grid is rebuilt from the observation (E40b).
    All,
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
            genes: Vec::new(),
            track: Vec::new(),
            beta: default_beta(),
            sigma: default_sigma(),
            immigrants: default_immigrants(),
            crossover: 0.0,
            immigrant_reset: false,
            immigrant_reset_gate: None,
            state_correction: StateCorrection::None,
            driver: None,
        }
    }
}

/// One member: its grid, its genome, its seed and its driver scratch.
#[derive(Clone, Debug)]
pub struct Member {
    pub sim: Sim,
    pub genome: Genome,
    pub seed: u64,
    pub state: MemberState,
}

/// What one round of learning did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssimilationReport {
    /// Each member's score before resampling (IoU for [`Ensemble::assimilate`]).
    pub scores: Vec<f64>,
    /// `(Σw)² / Σw²`: how many members effectively carried weight. Near 1
    /// means the population collapsed onto one member.
    pub effective_sample_size: f64,
    /// Which old member each new member descends from.
    pub parents: Vec<usize>,
    /// How many new members were fresh draws rather than children.
    pub immigrants: usize,
    /// Children whose mutated genome the grid refused, and which therefore
    /// kept their parent's genome unchanged.
    pub rejected: usize,
}

/// Many copies of one simulation, stepping together. See the module docs.
pub struct Ensemble {
    config: EnsembleConfig,
    space: GeneSpace,
    track: Vec<CellType>,
    members: Vec<Member>,
    rng: Rng,
    next_seed: u64,
    generation: usize,
    forcing: Forcing,
    total: usize,
    /// Mean member burned area over observed burned area at the last
    /// [`Ensemble::assimilate`] call; `None` until the first one. See
    /// [`EnsembleConfig::immigrant_reset_gate`] for why this lives here
    /// rather than being passed in from outside: the engine already holds
    /// both burned counts at scoring time, and computing the ratio right
    /// there is simpler than threading it through the caller.
    last_area_ratio: Option<f64>,
    /// The observation from the last [`Self::assimilate`] call, as a
    /// throwaway [`Sim`] (see [`MemberDriver::seed_from_observation`]);
    /// `None` until the first call, and never built at all when
    /// [`EnsembleConfig::state_correction`] is `None` (building it clones a
    /// member, so a config that never needs it never pays for it).
    /// A direct call to [`Self::assimilate_scores`] never updates it, same
    /// as `last_area_ratio`.
    last_observed: Option<Sim>,
}

impl std::fmt::Debug for Ensemble {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ensemble")
            .field("members", &self.members.len())
            .field("generation", &self.generation)
            .field("genes", &self.space.len())
            .finish()
    }
}

fn config_error(why: impl std::fmt::Display) -> ModelError {
    ModelError::InvalidParam(format!("ensemble: {why}"))
}

/// Check the numeric settings shared by ensembles and evolutions.
pub(crate) fn check_selection_settings(
    beta: f64,
    sigma: f64,
    immigrants: f64,
) -> Result<(), ModelError> {
    if !(beta.is_finite() && beta >= 0.0) {
        return Err(config_error("beta must be a number >= 0"));
    }
    if !(sigma.is_finite() && sigma > 0.0) {
        return Err(config_error("sigma must be a number > 0"));
    }
    if !(0.0..=1.0).contains(&immigrants) {
        return Err(config_error("immigrants must be between 0 and 1"));
    }
    Ok(())
}

/// Resolve the `track` names against a grid: empty means every declared type
/// except the background; anything unknown is refused.
pub(crate) fn resolve_track(names: &[String], sim: &Sim) -> Result<Vec<CellType>, ModelError> {
    let declared = sim.declared_types();
    if names.is_empty() {
        return Ok(declared
            .into_iter()
            .filter(|t| *t != sim.inactive())
            .collect());
    }
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        match declared.iter().find(|t| t.as_str() == name) {
            Some(t) => out.push(*t),
            None => {
                let known: Vec<&str> = declared.iter().map(|t| t.as_str()).collect();
                return Err(config_error(format!(
                    "track: '{name}' is not declared by this grid (declared: {})",
                    known.join(", ")
                )));
            }
        }
    }
    Ok(out)
}

impl Ensemble {
    /// Build the members from a template grid. The template's cells are every
    /// member's starting state; its knobs are the defaults the genes vary.
    pub fn new(template: Sim, cfg: &EnsembleConfig) -> Result<Self, ModelError> {
        if cfg.members == 0 {
            return Err(config_error("members must be at least 1"));
        }
        check_selection_settings(cfg.beta, cfg.sigma, cfg.immigrants)?;
        let (free, owned) = match &cfg.driver {
            Some(d) => (d.free_genes(), d.owned_keys()),
            None => (Vec::new(), Vec::new()),
        };
        let space = GeneSpace::resolve(&cfg.genes, &template, &free, &owned)?;
        let track = resolve_track(&cfg.track, &template)?;
        let total = template.len();
        let mut rng = Rng::new(0xC0FF_EE00 ^ cfg.seed.wrapping_mul(0x9E37_79B9));
        let base = cfg.seed.wrapping_mul(1_000_000);
        let forcing = Forcing::new();
        let mut members = Vec::with_capacity(cfg.members);
        for i in 0..cfg.members {
            let genome = space.sample(&mut rng);
            let seed = base + i as u64;
            let mut sim = template.clone();
            sim.set_seed(seed);
            let mut state = MemberState::default();
            space.apply(&mut sim, &genome)?;
            if let Some(d) = &cfg.driver {
                d.apply(&mut sim, &genome, &space, &forcing, &mut state)?;
            }
            members.push(Member {
                sim,
                genome,
                seed,
                state,
            });
        }
        Ok(Ensemble {
            config: cfg.clone(),
            space,
            track,
            members,
            rng,
            next_seed: base + cfg.members as u64,
            generation: 0,
            forcing,
            total,
            last_area_ratio: None,
            last_observed: None,
        })
    }

    /// Number of members.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// True for an ensemble with no members (never, after `new`).
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Rounds of learning so far.
    pub fn generation(&self) -> usize {
        self.generation
    }

    pub fn config(&self) -> &EnsembleConfig {
        &self.config
    }

    /// The resolved genes.
    pub fn space(&self) -> &GeneSpace {
        &self.space
    }

    /// The types the probability map counts as "on".
    pub fn track(&self) -> &[CellType] {
        &self.track
    }

    /// The members, for reading grids and genomes.
    pub fn members(&self) -> &[Member] {
        &self.members
    }

    /// The members, for painting every grid or reading a model.
    pub fn members_mut(&mut self) -> &mut [Member] {
        &mut self.members
    }

    /// Every member's genome as `key -> value`.
    pub fn genomes(&self) -> Vec<BTreeMap<String, ParamValue>> {
        self.members
            .iter()
            .map(|m| self.space.named(&m.genome))
            .collect()
    }

    /// Steps every member has taken (they always agree).
    pub fn step_count(&self) -> u64 {
        self.members.first().map_or(0, |m| m.sim.step_count())
    }

    /// The forcing currently in effect.
    pub fn forcing(&self) -> &Forcing {
        &self.forcing
    }

    /// Set the external inputs for the coming steps and let the driver apply
    /// them to every member. Without a driver the forcing is only stored.
    pub fn set_forcing(&mut self, forcing: Forcing) -> Result<(), ModelError> {
        self.forcing = forcing;
        if let Some(d) = &self.config.driver {
            for m in &mut self.members {
                d.apply(
                    &mut m.sim,
                    &m.genome,
                    &self.space,
                    &self.forcing,
                    &mut m.state,
                )?;
            }
        }
        Ok(())
    }

    /// Advance every member one step, then run the driver's period hook if
    /// this step completes a period.
    ///
    /// Members step in parallel when one member is too small to be split
    /// into chunks itself, and one after another (each using the engine's
    /// own chunk parallelism) otherwise — the result is the same either way.
    ///
    /// `CELLA_MEMBER_PAR=<n>` (see [`crate::threads`]) overrides that choice:
    /// members are split into batches of at most `n`, and each batch steps
    /// concurrently on the pool before the next batch starts. `n=1` forces
    /// the fully-sequential shape (one member at a time, each free to use
    /// every thread for its own chunking); `n >= members.len()` forces every
    /// member concurrently, same as the heuristic's parallel branch. Unset,
    /// this method is unchanged from before the knob existed — see the
    /// 2026-09-12 "Ensemble stepping parallelism" study in
    /// docs/performance.md for why a one-off study needed this at all.
    pub fn step(&mut self) -> Result<(), ModelError> {
        match member_par_override() {
            Some(n) => {
                for batch in self.members.chunks_mut(n) {
                    pool(thread_count()).install(|| {
                        batch.par_iter_mut().for_each(|m| m.sim.step());
                    });
                }
            }
            None => {
                let parallel_members =
                    self.members.len() > 1 && chunks_for_work(self.total.saturating_mul(12)) <= 1;
                if parallel_members {
                    pool(thread_count()).install(|| {
                        self.members.par_iter_mut().for_each(|m| m.sim.step());
                    });
                } else {
                    for m in &mut self.members {
                        m.sim.step();
                    }
                }
            }
        }
        if let Some(n) = self.config.driver.as_ref().and_then(|d| d.period_steps())
            && n > 0
            && self.step_count().is_multiple_of(n)
        {
            self.period_end()?;
            // A period boundary is also when a driver re-reads its own
            // schedule (a weather window that changed with the clock), so
            // the current forcing is applied again.
            self.reapply_driver()?;
        }
        Ok(())
    }

    /// Run the driver's `apply` on every member with the forcing as it is.
    fn reapply_driver(&mut self) -> Result<(), ModelError> {
        if let Some(d) = &self.config.driver {
            for m in &mut self.members {
                d.apply(
                    &mut m.sim,
                    &m.genome,
                    &self.space,
                    &self.forcing,
                    &mut m.state,
                )?;
            }
        }
        Ok(())
    }

    /// Advance every member `n` steps.
    pub fn step_n(&mut self, n: u64) -> Result<(), ModelError> {
        for _ in 0..n {
            self.step()?;
        }
        Ok(())
    }

    /// Run the driver's period hook on every member now (it also runs on its
    /// own when the driver declares a period). No-op without a driver.
    pub fn period_end(&mut self) -> Result<(), ModelError> {
        let Some(d) = &self.config.driver else {
            return Ok(());
        };
        for m in &mut self.members {
            d.period_end(
                &mut m.sim,
                &m.genome,
                &self.space,
                &mut m.state,
                &mut self.rng,
            )?;
        }
        Ok(())
    }

    /// One member's cells that are in any of `types`.
    pub fn member_mask(&self, i: usize, types: &[CellType]) -> Vec<bool> {
        self.members[i].sim.mask(types)
    }

    /// Per cell, the fraction of members in which the cell is in one of
    /// `types` (use [`Self::track`] for the configured set).
    pub fn state_probability(&self, types: &[CellType]) -> Vec<f32> {
        let mut p = vec![0.0f32; self.total];
        for m in &self.members {
            for (slot, c) in p.iter_mut().zip(m.sim.cells()) {
                if types.contains(c) {
                    *slot += 1.0;
                }
            }
        }
        let inv = 1.0 / self.members.len() as f32;
        for slot in &mut p {
            *slot *= inv;
        }
        p
    }

    /// Cells whose probability is at least `threshold` (0.5 = majority).
    pub fn consensus(&self, types: &[CellType], threshold: f32) -> Vec<bool> {
        self.state_probability(types)
            .iter()
            .map(|&p| p >= threshold)
            .collect()
    }

    /// Mean and standard deviation of any per-grid measurement over the
    /// members, e.g. `ens.metric_stats(|s| metrics::entropy(s))`.
    pub fn metric_stats(&self, f: impl Fn(&Sim) -> f64) -> (f64, f64) {
        let v: Vec<f64> = self.members.iter().map(|m| f(&m.sim)).collect();
        mean_sd(&v)
    }

    /// `(mean, sd, min, max)` of a numeric gene over the members; `None` for
    /// an unknown key or a choice/bits gene.
    pub fn genome_stats(&self, key: &str) -> Option<(f64, f64, f64, f64)> {
        let v: Vec<f64> = self
            .members
            .iter()
            .map(|m| self.space.float(&m.genome, key))
            .collect::<Option<Vec<f64>>>()?;
        if v.is_empty() {
            return None;
        }
        let (mean, sd) = mean_sd(&v);
        let min = v.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((mean, sd, min, max))
    }

    /// Fraction of members whose driver state has `key` set as a flag
    /// (e.g. `"contained"` for the wildfire driver).
    pub fn state_fraction(&self, key: &str) -> f64 {
        if self.members.is_empty() {
            return 0.0;
        }
        let n = self.members.iter().filter(|m| m.state.flag(key)).count();
        n as f64 / self.members.len() as f64
    }

    /// One round of learning from an observed mask: each member is scored by
    /// the IoU between its `types` mask and `observed`, then
    /// [`Self::assimilate_scores`] does the rest. Score your forecast against
    /// `observed` *before* calling this, never after.
    ///
    /// Along the way this also computes the *area ratio* — mean member
    /// burned area over observed burned area, the same number the wildfire
    /// runner reports as `area_ratio_mean` — and caches it for
    /// [`EnsembleConfig::immigrant_reset_gate`] to read inside
    /// [`Self::assimilate_scores`]. A direct call to `assimilate_scores`
    /// never updates it.
    pub fn assimilate(
        &mut self,
        observed: &[bool],
        types: &[CellType],
    ) -> Result<AssimilationReport, ModelError> {
        if observed.len() != self.total {
            return Err(ModelError::LayerLength {
                layer: "observed",
                expected: self.total,
                got: observed.len(),
            });
        }
        let obs_area = observed.iter().filter(|&&b| b).count() as f64;
        let mut scores = Vec::with_capacity(self.members.len());
        let mut area_sum = 0.0;
        for i in 0..self.members.len() {
            let mask = self.member_mask(i, types);
            scores.push(iou(&mask, observed));
            area_sum += mask.iter().filter(|&&b| b).count() as f64;
        }
        self.last_area_ratio = Some(area_sum / self.members.len() as f64 / obs_area.max(1.0));
        self.last_observed = (self.config.state_correction != StateCorrection::None)
            .then(|| self.observed_as_sim(observed, types));
        self.assimilate_scores(&scores)
    }

    /// A throwaway [`Sim`] carrying `observed` in this ensemble's own cell
    /// types, for [`MemberDriver::seed_from_observation`]: cell `i` is
    /// painted with `types[0]` (or this grid's own inactive type, if
    /// `types` is empty — `track` should never be, but this keeps the
    /// method total) when `observed[i]` is true, and with this grid's own
    /// inactive type otherwise. A driver reads it back with exactly that
    /// rule — "not this grid's inactive type" means "observed here" — so
    /// it never needs to know what `types[0]` means. Cloning a member is
    /// cheap next to a whole [`Self::new`], and keeps the engine from ever
    /// needing its own idea of "a grid": it borrows one of the model's.
    fn observed_as_sim(&self, observed: &[bool], types: &[CellType]) -> Sim {
        let mut sim = self.members[0].sim.clone();
        let inactive = sim.inactive();
        let on = types.first().copied().unwrap_or(inactive);
        let cells: Vec<CellType> = observed
            .iter()
            .map(|&b| if b { on } else { inactive })
            .collect();
        sim.reset_cells(cells)
            .expect("a member's own cell count always matches its own length");
        sim
    }

    /// One round of learning from any per-member score (higher is better):
    /// weight by `exp(beta × (score − best))`, resample systematically,
    /// mutate the children, replace a share with immigrants. Children keep
    /// their parent's grid state and driver scratch; only genomes and seeds
    /// change. A child whose mutated genome the grid refuses keeps its
    /// parent's genome (counted in `rejected`).
    pub fn assimilate_scores(&mut self, scores: &[f64]) -> Result<AssimilationReport, ModelError> {
        let m = self.members.len();
        if scores.len() != m {
            return Err(ModelError::LayerLength {
                layer: "scores",
                expected: m,
                got: scores.len(),
            });
        }
        let max = scores.iter().cloned().fold(f64::MIN, f64::max);
        let w: Vec<f64> = scores
            .iter()
            .map(|&v| (self.config.beta * (v - max)).exp())
            .collect();
        let wsum: f64 = w.iter().sum();
        let ess = wsum * wsum / w.iter().map(|x| x * x).sum::<f64>();
        // Systematic resampling: one stratified offset, then walk the
        // cumulative weights. Members with more weight get more children.
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
        // Whether *this generation's* immigrants get a fresh state, when
        // `state_correction` is `None` for them (see below). The gate, when
        // set, overrides the plain bool: it resets only while the
        // last-observed area ratio says the population is under-predicting
        // (see `EnsembleConfig::immigrant_reset_gate`). No ratio yet is
        // treated as "do not reset" — there is no evidence for it.
        let reset_immigrants = match self.config.immigrant_reset_gate {
            Some(gate) => self.last_area_ratio.is_some_and(|ratio| ratio < gate),
            None => self.config.immigrant_reset,
        };
        // `All` corrects every child's grid; `Immigrants` corrects only an
        // immigrant's; either way a corrected child always gets a fresh
        // state too, and the gate/bool above are never consulted for it
        // (see `EnsembleConfig::state_correction`).
        let correct_all = self.config.state_correction == StateCorrection::All;
        let correct_immigrants =
            matches!(self.config.state_correction, StateCorrection::Immigrants) || correct_all;

        // The first children of a parent clone its grid; the last one moves
        // it, saving one full copy per surviving parent.
        let mut uses = vec![0usize; m];
        for &p in &parents {
            uses[p] += 1;
        }
        let old = std::mem::take(&mut self.members);
        let parent_info: Vec<(Genome, MemberState)> = old
            .iter()
            .map(|p| (p.genome.clone(), p.state.clone()))
            .collect();
        let mut slots: Vec<Option<Member>> = old.into_iter().map(Some).collect();
        let mut children: Vec<Member> = Vec::with_capacity(m);
        let mut rejected = 0usize;
        for (ci, &pi) in parents.iter().enumerate() {
            self.next_seed += 1;
            let (parent_genome, parent_state) = &parent_info[pi];
            let genome = if ci < n_imm {
                self.space.sample(&mut self.rng)
            } else {
                let mut g = parent_genome.clone();
                // Crossover draws nothing when it is off, so a config
                // without it reproduces the pre-crossover results exactly.
                if self.config.crossover > 0.0 && self.rng.uniform() < self.config.crossover {
                    let other = parents[self.rng.below(m)];
                    g = self.space.crossover(&mut self.rng, &g, &parent_info[other].0);
                }
                self.space.mutate(&mut self.rng, &mut g, self.config.sigma);
                g
            };
            uses[pi] -= 1;
            let mut sim = if uses[pi] == 0 {
                slots[pi].take().expect("each parent is moved once").sim
            } else {
                slots[pi]
                    .as_ref()
                    .expect("parent still present")
                    .sim
                    .clone()
            };
            sim.set_seed(self.next_seed);
            let is_immigrant = ci < n_imm;
            let corrected = correct_all || (is_immigrant && correct_immigrants);
            let mut state = if corrected || (is_immigrant && reset_immigrants) {
                MemberState::default()
            } else {
                parent_state.clone()
            };
            let genome = match self.space.apply(&mut sim, &genome) {
                Ok(()) => genome,
                Err(_) => {
                    rejected += 1;
                    // The parent's genome was accepted once already.
                    self.space.apply(&mut sim, parent_genome)?;
                    parent_genome.clone()
                }
            };
            if let Some(d) = &self.config.driver {
                d.apply(&mut sim, &genome, &self.space, &self.forcing, &mut state)?;
                if corrected {
                    if let Some(observed) = &self.last_observed {
                        d.seed_from_observation(&mut sim, observed)?;
                    }
                    // No observation yet (a bare `assimilate_scores` call,
                    // or correction set before the first `assimilate`): no
                    // evidence to seed from, so the grid is left as the
                    // parent's, exactly like `None` — the same "no evidence
                    // yet" fallback `immigrant_reset_gate` uses.
                }
            }
            children.push(Member {
                sim,
                genome,
                seed: self.next_seed,
                state,
            });
        }
        self.members = children;
        self.generation += 1;
        Ok(AssimilationReport {
            scores: scores.to_vec(),
            effective_sample_size: ess,
            parents,
            immigrants: n_imm,
            rejected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid2d::Grid2D;
    use crate::rng::cell_rand;
    use crate::rules::{CountOp, Neighborhood2D, Rule2D, Rule2DSubrule};

    /// `cargo test` runs tests in one process with many threads; the
    /// thread-count, member-parallelism and min-work overrides below are
    /// process-global (see `crate::threads`), so any test that touches one
    /// must hold this lock for the tests to be safe to run concurrently.
    static GLOBAL_OVERRIDE_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Life with a chance that a birth is skipped, on a seeded random soup.
    fn soup(w: usize, h: usize) -> Sim {
        let alive = CellType::from("Alive");
        let dead = CellType::inactive();
        let sub = |cur, count, op, limit, out, r| {
            Rule2DSubrule::new(
                cur,
                alive,
                count,
                op,
                1,
                Neighborhood2D::Moore,
                out,
                r,
                limit,
            )
        };
        let rule = Rule2D {
            subrules: vec![
                sub(alive, 4, CountOp::Gt, None, dead, None),
                sub(alive, 2, CountOp::Gt, Some(3), alive, None),
                sub(dead, 3, CountOp::Eq, None, alive, Some(0.3)),
            ],
        };
        let cells = (0..w * h)
            .map(|i| {
                if cell_rand(5, 0, i as u64, 7) < 0.4 {
                    alive
                } else {
                    dead
                }
            })
            .collect();
        Sim::D2(Grid2D::new(w, h, 0, cells, rule))
    }

    fn cfg(members: usize) -> EnsembleConfig {
        EnsembleConfig {
            members,
            genes: vec![
                GeneSpec::range("rule.subrules[2].randomness", 0.0, 0.5),
                GeneSpec::range("rule.subrules[0].count", 3.0, 5.0),
            ],
            track: vec!["Alive".into()],
            ..EnsembleConfig::default()
        }
    }

    #[test]
    fn members_are_reproducible_and_drawn_inside_the_genes() {
        let e1 = Ensemble::new(soup(12, 12), &cfg(8)).unwrap();
        let e2 = Ensemble::new(soup(12, 12), &cfg(8)).unwrap();
        assert_eq!(e1.genomes(), e2.genomes(), "same seed, same draw");
        assert_eq!(e1.len(), 8);
        assert!(!e1.is_empty());
        assert_eq!(e1.generation(), 0);
        assert_eq!(e1.step_count(), 0);
        assert_eq!(e1.track(), &[CellType::from("Alive")]);
        assert_eq!(e1.space().len(), 2);
        for m in e1.members() {
            let r = e1
                .space()
                .float(&m.genome, "rule.subrules[2].randomness")
                .unwrap();
            assert!((0.0..=0.5).contains(&r));
            let c = e1
                .space()
                .float(&m.genome, "rule.subrules[0].count")
                .unwrap();
            assert!((3.0..=5.0).contains(&c));
            // The genome was written into the member's grid.
            assert_eq!(
                m.sim.get_param("rule.subrules[0].count"),
                Some(ParamValue::Int(c as i64))
            );
        }
        let seeds: Vec<u64> = e1.members().iter().map(|m| m.seed).collect();
        assert_eq!(seeds, (0..8).collect::<Vec<u64>>());
        let e3 = Ensemble::new(soup(12, 12), &EnsembleConfig { seed: 7, ..cfg(8) }).unwrap();
        assert_ne!(
            e1.genomes(),
            e3.genomes(),
            "a different seed draws differently"
        );
        assert!(e3.members()[0].seed >= 7_000_000);
        let (mean, sd, min, max) = e1.genome_stats("rule.subrules[2].randomness").unwrap();
        assert!(min <= mean && mean <= max && sd >= 0.0);
        assert_eq!(e1.genome_stats("nope"), None);
        assert!(format!("{e1:?}").contains("members: 8"));
    }

    #[test]
    fn state_probability_is_a_fraction_and_members_disagree_where_dice_differ() {
        let mut e = Ensemble::new(soup(16, 16), &cfg(8)).unwrap();
        let alive = CellType::from("Alive");
        let p0 = e.state_probability(&[alive]);
        assert!(p0.iter().all(|&p| p == 0.0 || p == 1.0), "identical starts");
        e.step_n(6).unwrap();
        assert_eq!(e.step_count(), 6);
        let p1 = e.state_probability(&[alive]);
        assert!(p1.iter().all(|&p| (0.0..=1.0).contains(&p)));
        assert!(
            p1.iter().any(|&p| p > 0.0 && p < 1.0),
            "stochastic births differ per member"
        );
        let cons = e.consensus(&[alive], 0.5);
        assert_eq!(cons.len(), 256);
        assert_eq!(
            cons.iter().filter(|b| **b).count(),
            p1.iter().filter(|&&p| p >= 0.5).count()
        );
        let (mean, sd) = e.metric_stats(|s| super::super::metrics::fraction(s, &[alive]));
        assert!(mean > 0.0 && sd >= 0.0);
        assert_eq!(e.state_fraction("anything"), 0.0, "no driver, no flags");
        // Without a driver the forcing is only stored and the period hook is a no-op.
        let mut f = Forcing::new();
        f.insert("hours".into(), 3.0);
        e.set_forcing(f.clone()).unwrap();
        assert_eq!(e.forcing(), &f);
        e.period_end().unwrap();
    }

    #[test]
    fn assimilation_resamples_toward_the_member_that_matches_and_admits_immigrants() {
        let c = EnsembleConfig {
            beta: 30.0,
            immigrants: 0.25,
            ..cfg(8)
        };
        let mut e = Ensemble::new(soup(16, 16), &c).unwrap();
        let alive = CellType::from("Alive");
        e.step_n(6).unwrap();
        // Pretend the observation is exactly member 0's state.
        let observed = e.member_mask(0, &[alive]);
        let before = e.genomes();
        let rep = e.assimilate(&observed, &[alive]).unwrap();
        assert_eq!(rep.scores.len(), 8);
        assert_eq!(rep.scores[0], 1.0);
        assert_eq!(rep.immigrants, 2);
        assert!(rep.effective_sample_size < 8.0, "weights are not flat");
        assert!(
            rep.parents.iter().filter(|&&p| p == 0).count() >= 4,
            "member 0 dominates: {:?}",
            rep.parents
        );
        assert_eq!(e.generation(), 1);
        assert_eq!(rep.rejected, 0);
        // Non-immigrant children of member 0 sit near its genome.
        let after = e.genomes();
        let key = "rule.subrules[2].randomness";
        let near = rep
            .parents
            .iter()
            .zip(after.iter())
            .skip(rep.immigrants)
            .filter(|(p, child)| {
                **p == 0
                    && matches!((&child[key], &before[0][key]), (ParamValue::Float(a), ParamValue::Float(b)) if (a - b).abs() < 0.25)
            })
            .count();
        assert!(near >= 3, "mutation is a nudge, not a re-draw");
        // The grid state carried over: every child is at step 6.
        assert_eq!(e.step_count(), 6);
        // Seeds are fresh and unique.
        let mut seeds: Vec<u64> = e.members().iter().map(|m| m.seed).collect();
        seeds.sort_unstable();
        seeds.dedup();
        assert_eq!(seeds.len(), 8);
        // Wrong lengths are refused.
        assert!(e.assimilate(&observed[..10], &[alive]).is_err());
        assert!(e.assimilate_scores(&[1.0, 2.0]).is_err());
        // Learning is reproducible.
        let mut a = Ensemble::new(soup(12, 12), &c).unwrap();
        let mut b = Ensemble::new(soup(12, 12), &c).unwrap();
        a.step_n(3).unwrap();
        b.step_n(3).unwrap();
        let obs = a.member_mask(1, &[alive]);
        let ra = a.assimilate(&obs, &[alive]).unwrap();
        let rb = b.assimilate(&obs, &[alive]).unwrap();
        assert_eq!(ra, rb);
        assert_eq!(a.genomes(), b.genomes());
        assert_eq!(a.members()[0].sim.cells(), b.members()[0].sim.cells());
    }

    #[test]
    fn stepping_is_identical_across_thread_counts() {
        let _guard = GLOBAL_OVERRIDE_GUARD.lock().unwrap();
        use crate::threads::{clear_thread_override, set_thread_override};
        let run = |threads: usize| {
            set_thread_override(threads);
            let mut e = Ensemble::new(soup(20, 20), &cfg(6)).unwrap();
            e.step_n(5).unwrap();
            let p = e.state_probability(&[CellType::from("Alive")]);
            clear_thread_override();
            p
        };
        assert_eq!(run(1), run(4));
    }

    /// The 2026-09-12 "Ensemble stepping parallelism" study (docs/performance.md)
    /// added `CELLA_MEMBER_PAR`/`CELLA_MIN_WORK` as ways to force how members
    /// batch for concurrent stepping. Same seed, same members, same answer is
    /// the engine's standing promise (see this module's doc comment) — these
    /// knobs must not be able to break it, on either axis or the two together.
    #[test]
    fn stepping_is_identical_across_member_par_and_min_work() {
        let _guard = GLOBAL_OVERRIDE_GUARD.lock().unwrap();
        use crate::threads::{
            MIN_WORK_PER_CHUNK, clear_member_par_override, clear_min_work_per_chunk_override,
            clear_thread_override, set_member_par_override, set_min_work_per_chunk_override,
            set_thread_override,
        };
        set_thread_override(4);
        let run = |member_par: Option<usize>, min_work: Option<usize>| {
            match member_par {
                Some(n) => set_member_par_override(n),
                None => clear_member_par_override(),
            }
            match min_work {
                Some(n) => set_min_work_per_chunk_override(n),
                None => clear_min_work_per_chunk_override(),
            }
            let mut e = Ensemble::new(soup(20, 20), &cfg(6)).unwrap();
            e.step_n(5).unwrap();
            e.state_probability(&[CellType::from("Alive")])
        };
        // Baseline: both knobs unset, i.e. today's size heuristic decides —
        // this must be the same run every one of the knobs below reproduces.
        let baseline = run(None, None);
        // member_par=1 (fully sequential batches) and member_par=thread_count()
        // (one batch, every member concurrent) must both match it...
        assert_eq!(run(Some(1), None), baseline);
        assert_eq!(run(Some(4), None), baseline);
        // ...and so must min_work alone (finer and coarser than the default)...
        assert_eq!(run(None, Some(1)), baseline);
        assert_eq!(run(None, Some(MIN_WORK_PER_CHUNK)), baseline);
        // ...and both knobs together, at every corner of the study's grid.
        assert_eq!(run(Some(1), Some(1)), baseline);
        assert_eq!(run(Some(4), Some(MIN_WORK_PER_CHUNK)), baseline);
        clear_member_par_override();
        clear_min_work_per_chunk_override();
        clear_thread_override();
    }

    #[test]
    fn config_is_validated_and_round_trips_json() {
        let s = soup(8, 8);
        assert!(
            Ensemble::new(
                s.clone(),
                &EnsembleConfig {
                    members: 0,
                    ..cfg(1)
                }
            )
            .is_err()
        );
        assert!(
            Ensemble::new(
                s.clone(),
                &EnsembleConfig {
                    immigrants: 1.5,
                    ..cfg(2)
                }
            )
            .is_err()
        );
        assert!(
            Ensemble::new(
                s.clone(),
                &EnsembleConfig {
                    sigma: 0.0,
                    ..cfg(2)
                }
            )
            .is_err()
        );
        assert!(
            Ensemble::new(
                s.clone(),
                &EnsembleConfig {
                    beta: f64::NAN,
                    ..cfg(2)
                }
            )
            .is_err()
        );
        let err = Ensemble::new(
            s.clone(),
            &EnsembleConfig {
                track: vec!["Ghost".into()],
                ..cfg(2)
            },
        )
        .unwrap_err();
        assert!(format!("{err}").contains("Ghost"), "{err}");
        let err = Ensemble::new(
            s.clone(),
            &EnsembleConfig {
                genes: vec![GeneSpec::new("wind_scale")],
                ..cfg(2)
            },
        )
        .unwrap_err();
        assert!(format!("{err}").contains("wind_scale"), "{err}");
        // Empty track means every declared type but the background.
        let e = Ensemble::new(
            s.clone(),
            &EnsembleConfig {
                track: vec![],
                ..cfg(2)
            },
        )
        .unwrap();
        assert_eq!(e.track(), &[CellType::from("Alive")]);
        // Genes may be empty: members differ only by seed.
        let e = Ensemble::new(
            s,
            &EnsembleConfig {
                genes: vec![],
                ..cfg(2)
            },
        )
        .unwrap();
        assert!(e.space().is_empty());

        let c: EnsembleConfig = serde_json::from_str(
            r#"{"members": 4, "genes": [{"key": "rule.subrules[0].count", "range": [3, 5]}]}"#,
        )
        .unwrap();
        assert_eq!(c.members, 4);
        assert_eq!(c.beta, 10.0);
        assert_eq!(c.sigma, 0.2);
        assert_eq!(c.immigrants, 0.2);
        assert!(c.driver.is_none());
        let back: EnsembleConfig =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.genes, c.genes);
        assert!(
            serde_json::from_str::<EnsembleConfig>(r#"{"prior": {"p0": [0.1, 0.5]}}"#).is_err(),
            "the old prior block is refused"
        );
        assert!(serde_json::from_str::<EnsembleConfig>(r#"{"members": "many"}"#).is_err());
    }

    #[test]
    fn crossover_mixes_two_parents_and_is_off_by_default() {
        // Two genes, no mutation, no immigrants: every child gene must equal
        // some parent's value, and with crossover on some child must take one
        // gene from each of two different parents.
        let base = EnsembleConfig {
            seed: 3,
            // Tiny, not zero (zero is refused): an integer nudge rounds to 0.
            sigma: 1e-9,
            immigrants: 0.0,
            beta: 0.0,
            ..cfg(16)
        };
        // The tiny sigma still moves a float by ~1e-10, so compare loosely.
        let close = |a: &ParamValue, b: &ParamValue| match (a, b) {
            (ParamValue::Float(x), ParamValue::Float(y)) => (x - y).abs() < 1e-6,
            _ => a == b,
        };
        let same_genome = |a: &BTreeMap<String, ParamValue>, b: &BTreeMap<String, ParamValue>| {
            a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| close(v, w)))
        };
        let mut off = Ensemble::new(soup(8, 8), &base).unwrap();
        let before = off.genomes();
        off.assimilate_scores(&[1.0; 16]).unwrap();
        for g in off.genomes() {
            assert!(
                before.iter().any(|p| same_genome(p, &g)),
                "without crossover children are copies"
            );
        }
        let mut on = Ensemble::new(
            soup(8, 8),
            &EnsembleConfig {
                crossover: 1.0,
                ..base.clone()
            },
        )
        .unwrap();
        let parents = on.genomes();
        on.assimilate_scores(&[1.0; 16]).unwrap();
        let mut mixed = false;
        for g in on.genomes() {
            let a = parents
                .iter()
                .filter(|p| close(&p["rule.subrules[0].count"], &g["rule.subrules[0].count"]))
                .count();
            let b = parents
                .iter()
                .filter(|p| close(&p["rule.subrules[2].randomness"], &g["rule.subrules[2].randomness"]))
                .count();
            assert!(a > 0 && b > 0, "every gene comes from a parent");
            if !parents.iter().any(|p| same_genome(p, &g)) {
                mixed = true;
            }
        }
        assert!(mixed, "with crossover 1.0 some child is a new combination");
        // Off by default, and a config that says nothing about it parses.
        let cfg: EnsembleConfig = serde_json::from_str(r#"{"members": 4}"#).unwrap();
        assert_eq!(cfg.crossover, 0.0);
    }

    /// Build an 8-member ensemble, mark every member "contained" (as the
    /// wildfire driver would after a fire has stopped), and hand back its
    /// per-member ignition area so a test can build an `observed` mask with
    /// a chosen area ratio in mind.
    fn contained_population(c: &EnsembleConfig) -> (Ensemble, f64) {
        let mut e = Ensemble::new(soup(16, 16), c).unwrap();
        let alive = CellType::from("Alive");
        let area = e.member_mask(0, &[alive]).iter().filter(|&&b| b).count() as f64;
        assert!(area > 0.0, "the soup must not be empty for this test to mean anything");
        for m in e.members_mut() {
            m.state.set("contained", 1.0);
        }
        (e, area)
    }

    #[test]
    fn gate_below_one_resets_immigrants_even_though_they_were_contained() {
        let c = EnsembleConfig {
            beta: 0.0,
            immigrants: 0.25,
            immigrant_reset_gate: Some(1.0),
            ..cfg(8)
        };
        let (mut e, area) = contained_population(&c);
        let alive = CellType::from("Alive");
        // Observed area is 3x every member's: area ratio ~= 1/3, below the
        // gate, so the population is under-predicting what actually burned.
        let obs_area = ((area * 3.0).ceil() as usize).min(e.member_mask(0, &[alive]).len() - 1);
        let mut observed = vec![false; e.member_mask(0, &[alive]).len()];
        observed[..obs_area].fill(true);
        let rep = e.assimilate(&observed, &[alive]).unwrap();
        assert!(rep.immigrants >= 1, "the test needs at least one immigrant");
        for i in 0..rep.immigrants {
            assert!(
                !e.members()[i].state.flag("contained"),
                "immigrant {i} should have a fresh, uncontained state below the gate"
            );
        }
        // A non-immigrant child still inherits its parent's contained flag:
        // the gate only ever changes immigrants, never the rest.
        for i in rep.immigrants..e.len() {
            assert!(
                e.members()[i].state.flag("contained"),
                "non-immigrant {i} must keep inheriting parent state"
            );
        }
    }

    #[test]
    fn gate_above_one_leaves_immigrants_inheriting_contained() {
        let c = EnsembleConfig {
            beta: 0.0,
            immigrants: 0.25,
            // The bare bool says reset; the gate must override it to "no"
            // once the ratio sits above 1 — this is the Pier-cost fix E39
            // exists for.
            immigrant_reset: true,
            immigrant_reset_gate: Some(1.0),
            ..cfg(8)
        };
        let (mut e, area) = contained_population(&c);
        let alive = CellType::from("Alive");
        // Observed area is a third of every member's: area ratio ~= 3, above
        // the gate, so the population is not under-predicting: no reset.
        let obs_area = ((area / 3.0).ceil() as usize).max(1);
        let mut observed = vec![false; e.member_mask(0, &[alive]).len()];
        observed[..obs_area].fill(true);
        let rep = e.assimilate(&observed, &[alive]).unwrap();
        assert!(rep.immigrants >= 1, "the test needs at least one immigrant");
        for i in 0..e.len() {
            assert!(
                e.members()[i].state.flag("contained"),
                "member {i} should inherit contained: the gate found no evidence to reset"
            );
        }
    }

    #[test]
    fn gate_none_leaves_the_plain_bool_in_charge() {
        // gate None + immigrant_reset true must behave exactly like E38:
        // every immigrant resets, whatever the area ratio would have been.
        let c = EnsembleConfig {
            beta: 0.0,
            immigrants: 0.25,
            immigrant_reset: true,
            immigrant_reset_gate: None,
            ..cfg(8)
        };
        let (mut e, area) = contained_population(&c);
        let alive = CellType::from("Alive");
        // An observed area that would sit *above* the gate if one were set,
        // so this only passes if the gate is truly not consulted.
        let obs_area = ((area / 3.0).ceil() as usize).max(1);
        let mut observed = vec![false; e.member_mask(0, &[alive]).len()];
        observed[..obs_area].fill(true);
        let rep = e.assimilate(&observed, &[alive]).unwrap();
        assert!(rep.immigrants >= 1, "the test needs at least one immigrant");
        for i in 0..rep.immigrants {
            assert!(
                !e.members()[i].state.flag("contained"),
                "immigrant {i} must reset: immigrant_reset is true and no gate overrides it"
            );
        }

        // gate None + immigrant_reset false: nobody resets, area ratio or
        // not (this is the pre-E38, pre-E39 default).
        let c2 = EnsembleConfig {
            immigrant_reset: false,
            immigrant_reset_gate: None,
            ..c
        };
        let (mut e2, _) = contained_population(&c2);
        let rep2 = e2.assimilate(&observed, &[alive]).unwrap();
        for i in 0..rep2.immigrants {
            assert!(
                e2.members()[i].state.flag("contained"),
                "immigrant {i} must not reset: both the gate and the bool are off"
            );
        }
    }

    /// A driver that does nothing but satisfy the trait — enough to exercise
    /// [`MemberDriver::seed_from_observation`]'s *default* implementation at
    /// the engine level, without pulling in the wildfire model. The
    /// wildfire driver's own override (rim-marking) is tested separately in
    /// `wildfire::driver`, on real fuel/fire cell types.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NoopDriver;

    #[typetag::serde(name = "noop_test_driver")]
    impl MemberDriver for NoopDriver {
        fn apply(
            &self,
            _sim: &mut Sim,
            _genome: &Genome,
            _space: &GeneSpace,
            _forcing: &Forcing,
            _state: &mut MemberState,
        ) -> Result<(), ModelError> {
            Ok(())
        }

        fn boxed_clone(&self) -> Box<dyn MemberDriver> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn immigrants_correction_seeds_only_immigrants_and_none_is_unaffected() {
        let base = EnsembleConfig {
            beta: 0.0,
            immigrants: 0.25,
            driver: Some(Box::new(NoopDriver)),
            ..cfg(8)
        };
        let alive = CellType::from("Alive");
        let mut observed = vec![false; 16 * 16];
        observed[..40].fill(true);

        // Two ensembles, identical seed and config except `state_correction`.
        let mut prior = Ensemble::new(
            soup(16, 16),
            &EnsembleConfig {
                state_correction: StateCorrection::None,
                ..base.clone()
            },
        )
        .unwrap();
        prior.step_n(6).unwrap();
        let rep_p = prior.assimilate(&observed, &[alive]).unwrap();

        let mut obs = Ensemble::new(
            soup(16, 16),
            &EnsembleConfig {
                state_correction: StateCorrection::Immigrants,
                ..base
            },
        )
        .unwrap();
        obs.step_n(6).unwrap();
        let rep_o = obs.assimilate(&observed, &[alive]).unwrap();

        assert!(rep_o.immigrants >= 1, "the test needs at least one immigrant");
        // Seeding a grid draws no random numbers, so the two runs must
        // resample, mutate and admit immigrants identically.
        assert_eq!(rep_p.parents, rep_o.parents, "seeding must not perturb resampling");
        assert_eq!(
            prior.genomes(),
            obs.genomes(),
            "seeding must not touch a single gene"
        );

        // The default `seed_from_observation` copies the observation
        // verbatim, so a corrected immigrant's tracked mask must equal the
        // observation exactly.
        for i in 0..rep_o.immigrants {
            assert_eq!(
                obs.member_mask(i, &[alive]),
                observed,
                "corrected immigrant {i} must equal the observation exactly"
            );
        }
        // Every non-immigrant child, and every part of an `Immigrants` run
        // that `None` also produces, must be byte-for-byte the same run:
        // this is "None is unaffected" checked directly, not asserted.
        for i in rep_o.immigrants..obs.len() {
            assert_eq!(
                obs.members()[i].sim.cells(),
                prior.members()[i].sim.cells(),
                "non-immigrant {i} must be identical whether or not \
                 state_correction is Immigrants"
            );
        }

        // With no observation yet, an `Immigrants` config falls back to
        // leaving the immigrant's grid as the parent's — the same "no
        // evidence" fallback the reset gate uses (see
        // `EnsembleConfig::immigrant_reset_gate`) — rather than erroring.
        let mut fresh = Ensemble::new(
            soup(8, 8),
            &EnsembleConfig {
                members: 8,
                immigrants: 0.25,
                state_correction: StateCorrection::Immigrants,
                driver: Some(Box::new(NoopDriver)),
                ..cfg(8)
            },
        )
        .unwrap();
        let before = fresh.members()[0].sim.cells().to_vec();
        fresh.assimilate_scores(&vec![1.0; 8]).unwrap();
        assert_eq!(
            fresh.members()[0].sim.cells(),
            &before[..],
            "no observation yet: an immigrant's grid is left untouched"
        );
    }

    #[test]
    fn all_correction_seeds_every_member_and_keeps_learned_genomes() {
        // `All` must correct every child's grid (not just immigrants) while
        // leaving genomes exactly as ordinary resampling/mutation would —
        // the point of E40b: learning continues, only the grid is fixed.
        let base = EnsembleConfig {
            beta: 0.0,
            immigrants: 0.25,
            driver: Some(Box::new(NoopDriver)),
            ..cfg(8)
        };
        let alive = CellType::from("Alive");
        let mut observed = vec![false; 16 * 16];
        observed[..40].fill(true);

        let mut none_run = Ensemble::new(
            soup(16, 16),
            &EnsembleConfig {
                state_correction: StateCorrection::None,
                ..base.clone()
            },
        )
        .unwrap();
        none_run.step_n(6).unwrap();
        let rep_n = none_run.assimilate(&observed, &[alive]).unwrap();

        let mut all_run = Ensemble::new(
            soup(16, 16),
            &EnsembleConfig {
                state_correction: StateCorrection::All,
                ..base
            },
        )
        .unwrap();
        all_run.step_n(6).unwrap();
        let rep_a = all_run.assimilate(&observed, &[alive]).unwrap();

        // Genomes are still the resampled/mutated ones: identical resampling
        // and identical genomes between `None` and `All`, since seeding the
        // grid draws no random numbers and never touches a gene.
        assert_eq!(rep_n.parents, rep_a.parents);
        assert_eq!(
            none_run.genomes(),
            all_run.genomes(),
            "state correction must not change a single learned gene"
        );

        // Every member's burned set equals the observed mask exactly —
        // immigrant or not.
        for i in 0..all_run.len() {
            assert_eq!(
                all_run.member_mask(i, &[alive]),
                observed,
                "member {i} must equal the observation exactly under All"
            );
        }
        assert!(rep_a.immigrants >= 1, "the test needs at least one immigrant");
    }
}
