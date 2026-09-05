//! [`Evolution`]: breed genomes until they do what you asked — or until they
//! have shown you everything they can do.
//!
//! An evolution keeps a **population** of genomes. Each generation every
//! genome is **evaluated**: a fresh copy of the template grid gets the genome
//! written into its knobs, runs for `steps` steps (repeated over `repeats`
//! seeds, averaged), and is scored. Then a new population is bred from the
//! old one. Three ways to breed, chosen by [`Search`]:
//!
//! - **`objective`** — the classic genetic algorithm. Score = the
//!   [`Objective`]. Keep the `elite` best unchanged, draw `immigrants` fresh
//!   from the gene ranges, and fill the rest with children: pick two parents
//!   ([`Selection`]), cross them with probability `crossover`, nudge each gene
//!   with probability `mutation` (size `sigma`).
//! - **`novelty`** — the same loop, but the score used for breeding is how
//!   *different* a genome's behaviour is from everything seen so far (mean
//!   distance to its `k` nearest neighbours in descriptor space). Genomes
//!   novel enough join a growing archive. Lehman & Stanley's novelty search:
//!   reward being different, and interesting behaviours appear that a
//!   fitness function would never have asked for.
//! - **`map_elites`** — no population to speak of: an [`Archive`] with one
//!   cell per region of descriptor space. Each generation takes `batch`
//!   elites at random, mutates them, evaluates the children and offers each
//!   to its cell (kept if the cell is empty or it scores higher). With no
//!   objective every genome scores 1 and filling the archive is the whole
//!   point (illumination); with one, each cell holds the best genome that
//!   behaves that way.
//!
//! Every evaluation is seeded from `(seed, generation, index, repeat)`, and
//! only the breeding step uses the sequential random generator, so a run is
//! reproducible on any thread count.

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::archive::{
    Archive, ArchiveSnapshot, ArchiveStats, Descriptor, DescriptorSpec, Elite, Thumbnail,
    thumbnail_from_rows,
};
use super::driver::{Forcing, MemberDriver, MemberState};
use super::ensemble::check_selection_settings;
use super::genome::{GeneKind, GeneSpace, GeneSpec, Genome};
use super::metrics::{Fitness, Goal, Metric, Objective};
use super::sim::Sim;
use crate::external::{ModelError, ParamValue};
use crate::rng::{Rng, STREAM_FILL, cell_rand, mix};
use crate::threads::{pool, thread_count};
use crate::types::CellType;

/// How parents are picked for breeding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    /// Draw `k` genomes at random and take the best. `k = 3` is gentle;
    /// bigger is greedier.
    Tournament { k: usize },
    /// Pick in proportion to `exp(beta × score)`: the softer the `beta`,
    /// the flatter the odds.
    Boltzmann { beta: f64 },
}

impl Default for Selection {
    fn default() -> Self {
        Selection::Tournament { k: 3 }
    }
}

/// Where each evaluation starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitialCondition {
    /// The template grid's own cells, every time.
    #[default]
    Fixed,
    /// A fresh random fill each generation (the same one for every genome of
    /// that generation): each cell draws one of `types` with the given
    /// `weights` (equal when empty).
    Random {
        types: Vec<String>,
        #[serde(default)]
        weights: Vec<f64>,
    },
}

/// Which kind of search runs. See the module docs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Search {
    /// Maximise the objective.
    #[default]
    Objective,
    /// Reward behaviours far from everything seen so far.
    Novelty {
        /// Neighbours averaged for the novelty distance.
        #[serde(default = "default_k")]
        k: usize,
        /// Novelty needed to enter the archive; adapts on its own when
        /// left out.
        #[serde(default)]
        threshold: Option<f64>,
    },
    /// Fill an archive of the best genome per behaviour region.
    MapElites {
        /// Children evaluated per generation.
        #[serde(default = "default_batch")]
        batch: usize,
        /// Mutate along the line between two elites as well as around one
        /// (Vassiliades & Mouret's iso+line operator).
        #[serde(default)]
        iso_line: bool,
    },
}

fn default_k() -> usize {
    15
}
fn default_batch() -> usize {
    32
}

/// Settings for an evolution (the `"evolve"` block of a config).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolveConfig {
    /// Genomes per generation (children per generation for `map_elites`).
    #[serde(default = "default_population")]
    pub population: usize,
    /// How many generations [`Evolution::run`] runs by default.
    #[serde(default = "default_generations")]
    pub generations: usize,
    /// Fixes every draw. Same seed, same evolution.
    #[serde(default)]
    pub seed: u64,
    /// Which knobs vary (see [`super::genome`]).
    #[serde(default)]
    pub genes: Vec<GeneSpec>,
    /// What to maximise. Required unless `search` is `map_elites`, where
    /// leaving it out means "just fill the archive".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<Objective>,
    /// Objective (default), novelty or MAP-Elites.
    #[serde(default)]
    pub search: Search,
    /// Behaviour axes for novelty and MAP-Elites (1 to 3).
    #[serde(default)]
    pub descriptors: Vec<DescriptorSpec>,
    /// Keep a thumbnail of each archived genome's final grid.
    #[serde(default = "default_true")]
    pub thumbnails: bool,
    /// Steps each evaluation runs.
    #[serde(default = "default_steps")]
    pub steps: u64,
    /// Seeds per evaluation; the score is their mean.
    #[serde(default = "default_repeats")]
    pub repeats: usize,
    /// Best genomes copied unchanged into the next generation.
    #[serde(default = "default_elite")]
    pub elite: usize,
    /// Probability a child has two parents rather than one.
    #[serde(default = "default_crossover")]
    pub crossover: f64,
    /// Probability each gene of a child is nudged.
    #[serde(default = "default_mutation")]
    pub mutation: f64,
    /// Size of a nudge (see [`super::genome`]).
    #[serde(default = "default_sigma")]
    pub sigma: f64,
    /// Share of each generation drawn fresh from the gene ranges.
    #[serde(default = "default_immigrants")]
    pub immigrants: f64,
    #[serde(default)]
    pub selection: Selection,
    #[serde(default)]
    pub initial: InitialCondition,
    /// Optional model-specific behaviour (see [`MemberDriver`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<Box<dyn MemberDriver>>,
    /// Forcing handed to the driver for every evaluation.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub forcing: Forcing,
}

fn default_population() -> usize {
    24
}
fn default_generations() -> usize {
    30
}
fn default_true() -> bool {
    true
}
fn default_steps() -> u64 {
    100
}
fn default_repeats() -> usize {
    3
}
fn default_elite() -> usize {
    2
}
fn default_crossover() -> f64 {
    0.5
}
fn default_mutation() -> f64 {
    0.3
}
fn default_sigma() -> f64 {
    0.2
}
fn default_immigrants() -> f64 {
    0.1
}

impl Default for EvolveConfig {
    fn default() -> Self {
        EvolveConfig {
            population: default_population(),
            generations: default_generations(),
            seed: 0,
            genes: Vec::new(),
            objective: None,
            search: Search::Objective,
            descriptors: Vec::new(),
            thumbnails: true,
            steps: default_steps(),
            repeats: default_repeats(),
            elite: default_elite(),
            crossover: default_crossover(),
            mutation: default_mutation(),
            sigma: default_sigma(),
            immigrants: default_immigrants(),
            selection: Selection::default(),
            initial: InitialCondition::Fixed,
            driver: None,
            forcing: Forcing::new(),
        }
    }
}

/// One evaluated genome.
#[derive(Clone, Debug, PartialEq)]
pub struct Individual {
    pub genome: Genome,
    /// The number breeding maximises: the objective's score, or the novelty
    /// in novelty search. `-inf` for a genome the grid refused.
    pub fitness: f64,
    /// The objective's score (1 when there is no objective).
    pub score: f64,
    /// The objective's raw value (its metric before the goal is applied).
    pub value: f64,
    /// One number per descriptor axis (empty without descriptors).
    pub descriptor: Vec<f64>,
    pub thumbnail: Option<Thumbnail>,
    /// The grid refused this genome (or a driver did); it scores `-inf`.
    pub invalid: bool,
}

/// What one generation produced.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GenerationReport {
    pub generation: u64,
    /// Best, mean, standard deviation and worst *score* over the valid
    /// genomes evaluated this generation.
    pub best: f64,
    pub mean: f64,
    pub sd: f64,
    pub min: f64,
    /// The best genome's raw objective value.
    pub best_value: f64,
    pub best_genome: Genome,
    pub best_named: BTreeMap<String, ParamValue>,
    /// Genomes evaluated this generation.
    pub evaluations: usize,
    /// Of which the grid refused.
    pub invalid: usize,
    /// Best score ever seen.
    pub hall_of_fame_best: f64,
    /// Archive numbers (MAP-Elites only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ArchiveStats>,
    /// Mean novelty of this generation (novelty search only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub novelty_mean: Option<f64>,
    /// Size of the novelty archive (novelty search only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_size: Option<usize>,
}

/// One entry of the novelty archive.
#[derive(Clone, Debug)]
struct NoveltyPoint {
    normalised: Vec<f64>,
    genome: Genome,
}

struct NoveltyArchive {
    points: Vec<NoveltyPoint>,
    threshold: f64,
    seen: usize,
    cap: usize,
}

const NOVELTY_CAP: usize = 2000;
const HALL_OF_FAME: usize = 5;
const THUMB_SIDE: u32 = 64;

/// A population, its scores and its archives. See the module docs.
pub struct Evolution {
    config: EvolveConfig,
    template: Sim,
    space: GeneSpace,
    fitness: Option<Arc<dyn Fitness>>,
    descriptor: Option<Descriptor>,
    population: Vec<Individual>,
    hall_of_fame: Vec<Individual>,
    archive: Option<Archive>,
    novelty: Option<NoveltyArchive>,
    rng: Rng,
    generation: u64,
    evaluations: u64,
    reports: Vec<GenerationReport>,
}

impl std::fmt::Debug for Evolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Evolution")
            .field("generation", &self.generation)
            .field("population", &self.population.len())
            .field("genes", &self.space.len())
            .field("search", &self.config.search)
            .finish()
    }
}

fn config_error(why: impl std::fmt::Display) -> ModelError {
    ModelError::InvalidParam(format!("evolve: {why}"))
}

struct Evaluation {
    score: f64,
    value: f64,
    descriptor: Vec<f64>,
    thumbnail: Option<Thumbnail>,
    invalid: bool,
}

impl Evaluation {
    fn invalid(dims: usize) -> Self {
        Evaluation {
            score: f64::NEG_INFINITY,
            value: f64::NAN,
            descriptor: vec![0.0; dims],
            thumbnail: None,
            invalid: true,
        }
    }
}

impl Evolution {
    /// Set up a search from a template grid and a config, scoring with the
    /// config's objective.
    pub fn new(template: Sim, cfg: &EvolveConfig) -> Result<Self, ModelError> {
        let fitness: Option<Arc<dyn Fitness>> = match &cfg.objective {
            Some(o) => {
                o.validate(&template)
                    .map_err(|e| config_error(format!("objective: {e}")))?;
                Some(Arc::new(o.clone()))
            }
            None => None,
        };
        Self::with_fitness_opt(template, cfg, fitness)
    }

    /// Set up a search that scores with your own [`Fitness`] instead of the
    /// config's objective (the config's `objective` is ignored).
    pub fn with_fitness(
        template: Sim,
        cfg: &EvolveConfig,
        fitness: Arc<dyn Fitness>,
    ) -> Result<Self, ModelError> {
        Self::with_fitness_opt(template, cfg, Some(fitness))
    }

    fn with_fitness_opt(
        template: Sim,
        cfg: &EvolveConfig,
        fitness: Option<Arc<dyn Fitness>>,
    ) -> Result<Self, ModelError> {
        if cfg.population < 2 {
            return Err(config_error("population must be at least 2"));
        }
        if cfg.elite >= cfg.population {
            return Err(config_error("elite must be smaller than population"));
        }
        if cfg.steps == 0 {
            return Err(config_error("steps must be at least 1"));
        }
        if cfg.repeats == 0 {
            return Err(config_error("repeats must be at least 1"));
        }
        if !(0.0..=1.0).contains(&cfg.crossover) {
            return Err(config_error("crossover must be between 0 and 1"));
        }
        if !(0.0..=1.0).contains(&cfg.mutation) {
            return Err(config_error("mutation must be between 0 and 1"));
        }
        let beta = match cfg.selection {
            Selection::Boltzmann { beta } => beta,
            Selection::Tournament { k } => {
                if k == 0 {
                    return Err(config_error("tournament k must be at least 1"));
                }
                1.0
            }
        };
        check_selection_settings(beta, cfg.sigma, cfg.immigrants)
            .map_err(|e| config_error(format!("{e}").trim_start_matches("ensemble: ")))?;
        if fitness.is_none() && !matches!(cfg.search, Search::MapElites { .. }) {
            return Err(config_error(
                "an objective is required unless search is map_elites",
            ));
        }
        if let Search::Novelty { k, threshold } = &cfg.search {
            if *k == 0 {
                return Err(config_error("novelty k must be at least 1"));
            }
            if threshold.is_some_and(|t| t.is_nan() || t <= 0.0) {
                return Err(config_error("novelty threshold must be > 0"));
            }
        }
        if let Search::MapElites { batch, .. } = &cfg.search
            && *batch == 0
        {
            return Err(config_error("map_elites batch must be at least 1"));
        }
        let (free, owned) = match &cfg.driver {
            Some(d) => (d.free_genes(), d.owned_keys()),
            None => (Vec::new(), Vec::new()),
        };
        let space =
            GeneSpace::resolve(&cfg.genes, &template, &free, &owned).map_err(config_error)?;
        let needs_descriptor = !matches!(cfg.search, Search::Objective);
        let descriptor = if needs_descriptor || !cfg.descriptors.is_empty() {
            if cfg.descriptors.is_empty() {
                return Err(config_error(
                    "novelty and map_elites need 1 to 3 descriptors",
                ));
            }
            Some(
                Descriptor::resolve(&cfg.descriptors, &template, cfg.steps)
                    .map_err(config_error)?,
            )
        } else {
            None
        };
        if let InitialCondition::Random { types, weights } = &cfg.initial {
            if types.is_empty() {
                return Err(config_error("initial.random needs at least one type"));
            }
            let declared = template.declared_types();
            for t in types {
                if !declared.iter().any(|d| d.as_str() == t) {
                    return Err(config_error(format!(
                        "initial.random names type '{t}', which this grid does not declare"
                    )));
                }
            }
            if !weights.is_empty() {
                if weights.len() != types.len() {
                    return Err(config_error("initial.random needs one weight per type"));
                }
                if weights.iter().any(|w| w.is_nan() || *w < 0.0)
                    || weights.iter().sum::<f64>() <= 0.0
                {
                    return Err(config_error(
                        "initial.random weights must be >= 0 and not all zero",
                    ));
                }
            }
        }
        let mut rng = Rng::new(mix(cfg.seed ^ 0x00E7_017E_D000_0001));
        let archive = match (&cfg.search, &descriptor) {
            (Search::MapElites { .. }, Some(d)) => {
                let offset = match &cfg.objective {
                    Some(o) if fitness.is_some() => {
                        let r = o.metric.range(&template, cfg.steps);
                        match o.goal {
                            Goal::Maximise => r[0],
                            Goal::Minimise => -r[1],
                            Goal::Target(t) => -((t - r[0]).abs().max((t - r[1]).abs())),
                        }
                    }
                    _ => 0.0,
                };
                Some(Archive::new(d, offset))
            }
            _ => None,
        };
        let novelty = match (&cfg.search, &descriptor) {
            (Search::Novelty { threshold, .. }, Some(d)) => Some(NoveltyArchive {
                points: Vec::new(),
                threshold: threshold.unwrap_or(0.1 * (d.dims() as f64).sqrt()),
                seen: 0,
                cap: NOVELTY_CAP,
            }),
            _ => None,
        };
        let initial_count = match cfg.search {
            Search::MapElites { batch, .. } => batch,
            _ => cfg.population,
        };
        let population = (0..initial_count)
            .map(|_| unevaluated(space.sample(&mut rng)))
            .collect();
        Ok(Evolution {
            config: cfg.clone(),
            template,
            space,
            fitness,
            descriptor,
            population,
            hall_of_fame: Vec::new(),
            archive,
            novelty,
            rng,
            generation: 0,
            evaluations: 0,
            reports: Vec::new(),
        })
    }

    pub fn config(&self) -> &EvolveConfig {
        &self.config
    }

    pub fn space(&self) -> &GeneSpace {
        &self.space
    }

    /// Generations completed.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Total evaluations so far.
    pub fn evaluations(&self) -> u64 {
        self.evaluations
    }

    /// The current population (evaluated once a generation has run).
    pub fn population(&self) -> &[Individual] {
        &self.population
    }

    /// The best genomes ever evaluated, best first, distinct genomes only.
    pub fn hall_of_fame(&self) -> &[Individual] {
        &self.hall_of_fame
    }

    /// The best genome ever evaluated.
    pub fn best(&self) -> Option<&Individual> {
        self.hall_of_fame.first()
    }

    /// Every report so far, oldest first.
    pub fn reports(&self) -> &[GenerationReport] {
        &self.reports
    }

    /// The MAP-Elites archive, when that is the search.
    pub fn archive(&self) -> Option<&Archive> {
        self.archive.as_ref()
    }

    /// A viewer's copy of the archive.
    pub fn archive_snapshot(&self) -> Option<ArchiveSnapshot> {
        self.archive.as_ref().map(|a| a.snapshot(self.generation))
    }

    /// The resolved descriptor axes, if any.
    pub fn descriptor(&self) -> Option<&Descriptor> {
        self.descriptor.as_ref()
    }

    /// The novelty archive (novelty search only): each entry's descriptor,
    /// scaled to `[0, 1]` per axis, and its genome by name.
    pub fn novelty_archive(&self) -> Vec<(Vec<f64>, BTreeMap<String, ParamValue>)> {
        self.novelty
            .as_ref()
            .map(|n| {
                n.points
                    .iter()
                    .map(|p| (p.normalised.clone(), self.space.named(&p.genome)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The current novelty threshold (novelty search only).
    pub fn novelty_threshold(&self) -> Option<f64> {
        self.novelty.as_ref().map(|n| n.threshold)
    }

    /// A genome as `key -> value`.
    pub fn named(&self, genome: &Genome) -> BTreeMap<String, ParamValue> {
        self.space.named(genome)
    }

    /// Write the best genome into a grid.
    pub fn apply_best(&self, sim: &mut Sim) -> Result<(), ModelError> {
        let best = self
            .best()
            .ok_or_else(|| ModelError::InvalidParam("nothing evaluated yet".into()))?;
        self.space.apply(sim, &best.genome)
    }

    /// Write the elite of archive cell `cell` into a grid.
    pub fn apply_elite(&self, cell: usize, sim: &mut Sim) -> Result<(), ModelError> {
        let archive = self.archive.as_ref().ok_or_else(|| {
            ModelError::InvalidParam("no archive: search is not map_elites".into())
        })?;
        let elite = archive
            .get(cell)
            .ok_or_else(|| ModelError::InvalidParam(format!("archive cell {cell} is empty")))?;
        self.space.apply(sim, &elite.genome)
    }

    /// Run `generations` generations, calling `on_report` after each.
    pub fn run(
        &mut self,
        generations: usize,
        mut on_report: impl FnMut(&GenerationReport),
    ) -> Vec<GenerationReport> {
        let mut out = Vec::with_capacity(generations);
        for _ in 0..generations {
            let r = self.step_generation();
            on_report(&r);
            out.push(r);
        }
        out
    }

    /// Score one genome as generation `generation`, individual `index`
    /// would be scored (`-inf` if the grid refuses it).
    pub fn evaluate(&self, genome: &Genome, generation: u64, index: usize) -> f64 {
        self.evaluate_full(genome, generation, index).score
    }

    /// The starting cells for an evaluation, or `None` to keep the template's.
    fn initial_cells(&self, ic_seed: u64) -> Option<Vec<CellType>> {
        let n = self.template.len();
        if let Some(Metric::DensityClassification { types }) =
            self.config.objective.as_ref().map(|o| &o.metric)
            && self.fitness.is_some()
        {
            let (a, b) = (CellType::new(&types[0]), CellType::new(&types[1]));
            let density = Rng::new(mix(ic_seed ^ 0xD3)).uniform();
            return Some(
                (0..n)
                    .map(|i| {
                        if f64::from(cell_rand(ic_seed, 0, i as u64, STREAM_FILL)) < density {
                            a
                        } else {
                            b
                        }
                    })
                    .collect(),
            );
        }
        match &self.config.initial {
            InitialCondition::Fixed => None,
            InitialCondition::Random { types, weights } => {
                let types: Vec<CellType> = types.iter().map(|t| CellType::new(t)).collect();
                let w: Vec<f64> = if weights.is_empty() {
                    vec![1.0; types.len()]
                } else {
                    weights.clone()
                };
                let total: f64 = w.iter().sum();
                let mut cum = Vec::with_capacity(w.len());
                let mut acc = 0.0;
                for x in &w {
                    acc += x / total;
                    cum.push(acc);
                }
                Some(
                    (0..n)
                        .map(|i| {
                            let u = f64::from(cell_rand(ic_seed, 0, i as u64, STREAM_FILL));
                            let pos = cum.iter().position(|c| u < *c).unwrap_or(types.len() - 1);
                            types[pos]
                        })
                        .collect(),
                )
            }
        }
    }

    fn evaluate_full(&self, genome: &Genome, generation: u64, index: usize) -> Evaluation {
        let dims = self.descriptor.as_ref().map_or(0, |d| d.dims());
        let repeats = self.config.repeats.max(1);
        let steps = self.config.steps;
        let want_thumbs =
            self.config.thumbnails && !matches!(self.config.search, Search::Objective);
        let mut score_sum = 0.0;
        let mut value_sum = 0.0;
        let mut desc_sum = vec![0.0; dims];
        let mut thumbnail = None;
        for r in 0..repeats as u64 {
            let mut sim = self.template.clone();
            let ic_seed = mix(self.config.seed ^ (generation << 32) ^ r ^ 0x1C);
            if let Some(cells) = self.initial_cells(ic_seed)
                && sim.reset_cells(cells).is_err()
            {
                return Evaluation::invalid(dims);
            }
            sim.set_seed(mix(self.config.seed
                ^ (generation << 40)
                ^ ((index as u64) << 20)
                ^ r));
            if self.space.apply(&mut sim, genome).is_err() {
                return Evaluation::invalid(dims);
            }
            let mut state = MemberState::default();
            if let Some(d) = &self.config.driver
                && d.apply(
                    &mut sim,
                    genome,
                    &self.space,
                    &self.config.forcing,
                    &mut state,
                )
                .is_err()
            {
                return Evaluation::invalid(dims);
            }
            let obj_every = self.fitness.as_ref().is_some_and(|f| f.every_step());
            let mut obj_samples: Vec<f64> = match &self.fitness {
                Some(f) => vec![f.sample(&sim)],
                None => Vec::new(),
            };
            let specs: &[DescriptorSpec] =
                self.descriptor.as_ref().map_or(&[], |d| d.specs.as_slice());
            let desc_every: Vec<bool> = specs
                .iter()
                .map(|s| {
                    s.metric.needs_every_step() || !matches!(s.when, super::metrics::When::End)
                })
                .collect();
            let mut desc_samples: Vec<Vec<f64>> =
                specs.iter().map(|s| vec![s.metric.sample(&sim)]).collect();
            let is_1d = matches!(sim, Sim::D1(_));
            let record_thumb = want_thumbs && r == repeats as u64 - 1;
            let row_every = steps.div_ceil(u64::from(THUMB_SIDE)).max(1);
            let mut rows: Vec<Vec<CellType>> = Vec::new();
            if record_thumb && is_1d {
                rows.push(sim.cells().to_vec());
            }
            let mut period_rng = Rng::new(mix(self.config.seed
                ^ generation
                ^ ((index as u64) << 8)
                ^ (r << 4)
                ^ 0x9E));
            let period = self.config.driver.as_ref().and_then(|d| d.period_steps());
            for t in 1..=steps {
                sim.step();
                if let (Some(d), Some(n)) = (&self.config.driver, period)
                    && n > 0
                    && t.is_multiple_of(n)
                    && d.period_end(&mut sim, genome, &self.space, &mut state, &mut period_rng)
                        .is_err()
                {
                    return Evaluation::invalid(dims);
                }
                if obj_every && let Some(f) = &self.fitness {
                    obj_samples.push(f.sample(&sim));
                }
                for (i, s) in specs.iter().enumerate() {
                    if desc_every[i] {
                        desc_samples[i].push(s.metric.sample(&sim));
                    }
                }
                if record_thumb && is_1d && t.is_multiple_of(row_every) {
                    rows.push(sim.cells().to_vec());
                }
            }
            if let Some(f) = &self.fitness {
                if !obj_every {
                    obj_samples.push(f.sample(&sim));
                }
                score_sum += f.score(&obj_samples);
                value_sum += f.aggregate(&obj_samples);
            } else {
                score_sum += 1.0;
                value_sum += 1.0;
            }
            for (i, s) in specs.iter().enumerate() {
                if !desc_every[i] {
                    desc_samples[i].push(s.metric.sample(&sim));
                }
                desc_sum[i] += s.metric.aggregate(&desc_samples[i], s.when);
            }
            if record_thumb {
                thumbnail = Some(if is_1d {
                    thumbnail_from_rows(sim.width(), &rows, THUMB_SIDE)
                } else {
                    sim.thumbnail(THUMB_SIDE)
                });
            }
        }
        let n = repeats as f64;
        Evaluation {
            score: score_sum / n,
            value: value_sum / n,
            descriptor: desc_sum.iter().map(|d| d / n).collect(),
            thumbnail,
            invalid: false,
        }
    }

    /// Evaluate the genomes that have not been scored yet, in parallel and
    /// in index order.
    fn evaluate_population(&mut self) {
        let generation = self.generation;
        let genomes: Vec<Genome> = self.population.iter().map(|i| i.genome.clone()).collect();
        let evals: Vec<Evaluation> = pool(thread_count()).install(|| {
            genomes
                .par_iter()
                .enumerate()
                .map(|(i, g)| self.evaluate_full(g, generation, i))
                .collect()
        });
        self.evaluations += evals.len() as u64;
        for (ind, e) in self.population.iter_mut().zip(evals) {
            ind.fitness = e.score;
            ind.score = e.score;
            ind.value = e.value;
            ind.descriptor = e.descriptor;
            ind.thumbnail = e.thumbnail;
            ind.invalid = e.invalid;
        }
    }

    fn update_hall_of_fame(&mut self) {
        let mut all: Vec<Individual> = self.hall_of_fame.drain(..).collect();
        all.extend(self.population.iter().filter(|i| !i.invalid).cloned());
        all.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut kept: Vec<Individual> = Vec::new();
        for ind in all {
            if !kept.iter().any(|k| k.genome == ind.genome) {
                kept.push(ind);
            }
            if kept.len() == HALL_OF_FAME {
                break;
            }
        }
        self.hall_of_fame = kept;
    }

    /// Score the current population, update the archives and the hall of
    /// fame, report, and breed the next population.
    pub fn step_generation(&mut self) -> GenerationReport {
        self.evaluate_population();
        let mut novelty_mean = None;
        let mut archive_size = None;
        match &self.config.search {
            Search::Objective => {}
            Search::Novelty { k, .. } => {
                let k = *k;
                let (mean, size) = self.score_novelty(k);
                novelty_mean = Some(mean);
                archive_size = Some(size);
            }
            Search::MapElites { .. } => self.offer_to_archive(),
        }
        self.update_hall_of_fame();
        let report = self.report(novelty_mean, archive_size);
        self.reports.push(report.clone());
        self.population = self.breed();
        self.generation += 1;
        report
    }

    fn report(&self, novelty_mean: Option<f64>, archive_size: Option<usize>) -> GenerationReport {
        let valid: Vec<&Individual> = self.population.iter().filter(|i| !i.invalid).collect();
        let scores: Vec<f64> = valid.iter().map(|i| i.score).collect();
        let (mean, sd) = super::metrics::mean_sd(&scores);
        let best_ind = valid
            .iter()
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .or_else(|| self.population.first());
        let (best_genome, best_value, best) = match best_ind {
            Some(i) => (i.genome.clone(), i.value, i.score),
            None => (Genome(Vec::new()), f64::NAN, f64::NEG_INFINITY),
        };
        GenerationReport {
            generation: self.generation,
            best,
            mean,
            sd,
            min: scores.iter().cloned().fold(f64::INFINITY, f64::min),
            best_value,
            best_named: self.space.named(&best_genome),
            best_genome,
            evaluations: self.population.len(),
            invalid: self.population.iter().filter(|i| i.invalid).count(),
            hall_of_fame_best: self.best().map_or(f64::NEG_INFINITY, |b| b.score),
            archive: self.archive.as_ref().map(|a| a.stats()),
            novelty_mean,
            archive_size,
        }
    }

    /// Replace each valid individual's fitness by its novelty and grow the
    /// novelty archive. Returns the mean novelty and the archive size.
    fn score_novelty(&mut self, k: usize) -> (f64, usize) {
        let desc = self.descriptor.as_ref().expect("novelty has descriptors");
        let arch = self.novelty.as_mut().expect("novelty archive");
        let pop_norm: Vec<Option<Vec<f64>>> = self
            .population
            .iter()
            .map(|i| (!i.invalid).then(|| desc.normalise(&i.descriptor)))
            .collect();
        let mut pool: Vec<&Vec<f64>> = pop_norm.iter().flatten().collect();
        pool.extend(arch.points.iter().map(|p| &p.normalised));
        let mut novelties = Vec::with_capacity(self.population.len());
        for me in &pop_norm {
            let Some(me) = me else {
                novelties.push(f64::NEG_INFINITY);
                continue;
            };
            let mut dists: Vec<f64> = pool
                .iter()
                .map(|other| {
                    me.iter()
                        .zip(other.iter())
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum::<f64>()
                        .sqrt()
                })
                .collect();
            dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            // The first distance is the point itself (0).
            let neigh: Vec<f64> = dists.into_iter().skip(1).take(k).collect();
            let nov = if neigh.is_empty() {
                0.0
            } else {
                neigh.iter().sum::<f64>() / neigh.len() as f64
            };
            novelties.push(nov);
        }
        let mut admitted = 0usize;
        for (i, nov) in novelties.iter().enumerate() {
            let ind = &mut self.population[i];
            ind.fitness = *nov;
            if ind.invalid {
                continue;
            }
            if *nov > arch.threshold {
                admitted += 1;
                arch.seen += 1;
                let point = NoveltyPoint {
                    normalised: pop_norm[i].clone().expect("valid"),
                    genome: ind.genome.clone(),
                };
                if arch.points.len() < arch.cap {
                    arch.points.push(point);
                } else {
                    // Reservoir replacement keeps the archive a fair sample.
                    let slot = self.rng.below(arch.seen);
                    if slot < arch.cap {
                        arch.points[slot] = point;
                    }
                }
            }
        }
        let valid = novelties.iter().filter(|n| n.is_finite()).count();
        if admitted > valid / 10 {
            arch.threshold *= 1.2;
        } else if admitted == 0 {
            arch.threshold *= 0.95;
        }
        let finite: Vec<f64> = novelties
            .iter()
            .cloned()
            .filter(|n| n.is_finite())
            .collect();
        let mean = if finite.is_empty() {
            0.0
        } else {
            finite.iter().sum::<f64>() / finite.len() as f64
        };
        (mean, arch.points.len())
    }

    /// Offer every valid individual to its archive cell.
    fn offer_to_archive(&mut self) {
        let archive = self.archive.as_mut().expect("map_elites has an archive");
        for ind in &self.population {
            if ind.invalid {
                continue;
            }
            archive.add(Elite {
                genome: ind.genome.clone(),
                named: self.space.named(&ind.genome),
                fitness: ind.score,
                descriptor: ind.descriptor.clone(),
                thumbnail: ind.thumbnail.clone(),
                generation: self.generation,
            });
        }
    }

    /// Pick one parent from the evaluated population.
    fn select(&mut self, order: &[usize]) -> usize {
        match self.config.selection {
            Selection::Tournament { k } => {
                let mut best: Option<usize> = None;
                for _ in 0..k.max(1) {
                    let cand = order[self.rng.below(order.len())];
                    best = Some(match best {
                        None => cand,
                        Some(b) if self.population[cand].fitness > self.population[b].fitness => {
                            cand
                        }
                        Some(b) => b,
                    });
                }
                best.expect("k >= 1")
            }
            Selection::Boltzmann { beta } => {
                let max = order
                    .iter()
                    .map(|&i| self.population[i].fitness)
                    .fold(f64::NEG_INFINITY, f64::max);
                let w: Vec<f64> = order
                    .iter()
                    .map(|&i| (beta * (self.population[i].fitness - max)).exp())
                    .collect();
                let total: f64 = w.iter().sum();
                let u = self.rng.uniform() * total;
                let mut acc = 0.0;
                for (j, wi) in w.iter().enumerate() {
                    acc += wi;
                    if u < acc {
                        return order[j];
                    }
                }
                order[order.len() - 1]
            }
        }
    }

    /// The next population, unevaluated.
    fn breed(&mut self) -> Vec<Individual> {
        match self.config.search {
            Search::MapElites { batch, iso_line } => self.emit_children(batch, iso_line),
            _ => self.breed_population(),
        }
    }

    fn breed_population(&mut self) -> Vec<Individual> {
        let n = self.config.population;
        // Valid genomes, best first (ties by index), for elites and parents.
        let mut order: Vec<usize> = (0..self.population.len())
            .filter(|&i| !self.population[i].invalid)
            .collect();
        order.sort_by(|&a, &b| {
            self.population[b]
                .fitness
                .partial_cmp(&self.population[a].fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        let mut next: Vec<Individual> = Vec::with_capacity(n);
        for &i in order.iter().take(self.config.elite) {
            next.push(unevaluated(self.population[i].genome.clone()));
        }
        let n_imm = ((self.config.immigrants * n as f64).round() as usize).min(n - next.len());
        for _ in 0..n_imm {
            let g = self.space.sample(&mut self.rng);
            next.push(unevaluated(g));
        }
        if order.is_empty() {
            // Nothing valid to breed from: start over from the gene ranges.
            while next.len() < n {
                let g = self.space.sample(&mut self.rng);
                next.push(unevaluated(g));
            }
            return next;
        }
        while next.len() < n {
            let a = self.select(&order);
            let b = self.select(&order);
            let mut child = if self.rng.uniform() < self.config.crossover {
                self.space.crossover(
                    &mut self.rng,
                    &self.population[a].genome,
                    &self.population[b].genome,
                )
            } else {
                self.population[a].genome.clone()
            };
            for gi in 0..self.space.len() {
                if self.rng.uniform() < self.config.mutation {
                    self.space
                        .mutate_one(&mut self.rng, &mut child, gi, self.config.sigma);
                }
            }
            next.push(unevaluated(child));
        }
        next
    }

    /// MAP-Elites emission: mutate random elites (or sample the gene ranges
    /// while the archive is still empty).
    fn emit_children(&mut self, batch: usize, iso_line: bool) -> Vec<Individual> {
        let archive = self.archive.as_ref().expect("map_elites has an archive");
        let elites: Vec<Genome> = archive.elites().map(|(_, e)| e.genome.clone()).collect();
        let mut next = Vec::with_capacity(batch);
        if elites.is_empty() {
            for _ in 0..batch {
                let g = self.space.sample(&mut self.rng);
                next.push(unevaluated(g));
            }
            return next;
        }
        for _ in 0..batch {
            let a = &elites[self.rng.below(elites.len())];
            let mut child = a.clone();
            if iso_line && elites.len() > 1 {
                let b = &elites[self.rng.below(elites.len())];
                iso_line_step(&self.space, &mut child, b, self.config.sigma, &mut self.rng);
            } else {
                self.space
                    .mutate(&mut self.rng, &mut child, self.config.sigma);
            }
            self.space.clamp(&mut child);
            next.push(unevaluated(child));
        }
        next
    }
}

fn unevaluated(genome: Genome) -> Individual {
    Individual {
        genome,
        fitness: f64::NAN,
        score: f64::NAN,
        value: f64::NAN,
        descriptor: Vec::new(),
        thumbnail: None,
        invalid: false,
    }
}

/// Iso+line variation: a Gaussian step around `child` plus a step along the
/// line towards `other` for every numeric gene; other kinds get the plain
/// mutation.
fn iso_line_step(space: &GeneSpace, child: &mut Genome, other: &Genome, sigma: f64, rng: &mut Rng) {
    for (i, gene) in space.genes().iter().enumerate() {
        let sigma_i = gene.sigma.unwrap_or(sigma);
        match (&gene.kind, &child.0[i], &other.0[i]) {
            (GeneKind::Float { lo, hi, log }, ParamValue::Float(a), ParamValue::Float(b)) => {
                let line = 0.2 * rng.normal();
                let v = if *log {
                    (a.ln() + sigma_i * rng.normal() + line * (b.ln() - a.ln())).exp()
                } else {
                    a + sigma_i * (hi - lo) * rng.normal() + line * (b - a)
                };
                child.0[i] = ParamValue::Float(v.clamp(*lo, *hi));
            }
            (GeneKind::Int { lo, hi }, ParamValue::Int(a), ParamValue::Int(b)) => {
                let step = sigma_i * (*hi - *lo) as f64 * rng.normal()
                    + 0.2 * rng.normal() * (*b - *a) as f64;
                child.0[i] = ParamValue::Int((*a + step.round() as i64).clamp(*lo, *hi));
            }
            _ => space.mutate_one(rng, child, i, sigma_i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explore::metrics::When;
    use crate::grid1d::Grid1D;
    use crate::grid2d::Grid2D;
    use crate::rules::{CountOp, Neighborhood2D, Rule1D, Rule1DSubrule, Rule2D, Rule2DSubrule};
    use crate::threads::{clear_thread_override, set_thread_override};

    fn life_soup(w: usize, h: usize) -> Sim {
        let alive = CellType::from("Alive");
        let dead = CellType::inactive();
        let sub = |cur, count, op, limit, out| {
            Rule2DSubrule::new(
                cur,
                alive,
                count,
                op,
                1,
                Neighborhood2D::Moore,
                out,
                None,
                limit,
            )
        };
        let rule = Rule2D {
            subrules: vec![
                sub(alive, 4, CountOp::Gt, None, dead),
                sub(alive, 2, CountOp::Gt, Some(3), alive),
                sub(dead, 3, CountOp::Eq, None, alive),
            ],
        };
        let cells = (0..w * h)
            .map(|i| {
                if cell_rand(11, 0, i as u64, 3) < 0.35 {
                    alive
                } else {
                    dead
                }
            })
            .collect();
        Sim::D2(Grid2D::new(w, h, 0, cells, rule))
    }

    fn row(width: usize) -> Sim {
        let x = CellType::from("X");
        let mk = |cur| Rule1DSubrule {
            current_type: cur,
            criteria_type: x,
            wolfram_code: 30,
            n: 1,
            randomness: None,
            output_type: x,
        };
        let rule = Rule1D {
            subrules: vec![mk(x), mk(CellType::inactive())],
        };
        let mut cells = vec![CellType::inactive(); width];
        cells[width / 2] = x;
        Sim::D1(Grid1D::new(width, 0, cells, rule))
    }

    fn life_genes() -> Vec<GeneSpec> {
        vec![
            GeneSpec::range("rule.subrules[0].count", 3.0, 8.0),
            GeneSpec::range("rule.subrules[1].count", 1.0, 4.0),
            GeneSpec::range("rule.subrules[2].count", 1.0, 6.0),
        ]
    }

    fn fraction_target(target: f64) -> Objective {
        Objective {
            metric: Metric::Fraction {
                types: vec!["Alive".into()],
            },
            goal: Goal::Target(target),
            when: When::End,
        }
    }

    fn small(cfg: EvolveConfig) -> EvolveConfig {
        EvolveConfig {
            population: 8,
            steps: 12,
            repeats: 1,
            ..cfg
        }
    }

    #[test]
    fn objective_search_improves_and_keeps_its_elites() {
        let cfg = small(EvolveConfig {
            genes: life_genes(),
            objective: Some(fraction_target(0.3)),
            seed: 3,
            ..EvolveConfig::default()
        });
        let mut evo = Evolution::new(life_soup(12, 12), &cfg).unwrap();
        assert_eq!(evo.population().len(), 8);
        assert!(evo.best().is_none());
        assert!(evo.apply_best(&mut life_soup(12, 12)).is_err());
        let reports = evo.run(6, |_| {});
        assert_eq!(reports.len(), 6);
        assert_eq!(evo.generation(), 6);
        assert_eq!(evo.evaluations(), 48);
        assert_eq!(evo.reports().len(), 6);
        for (g, r) in reports.iter().enumerate() {
            assert_eq!(r.generation as usize, g);
            assert_eq!(r.evaluations, 8);
            assert!(r.best <= 0.0, "a target score is never above 0: {}", r.best);
            assert!(r.min <= r.mean && r.mean <= r.best);
            assert!(r.archive.is_none() && r.novelty_mean.is_none());
            assert_eq!(r.best_named.len(), 3);
        }
        // Elitism: the best score never gets worse from one generation to the next.
        for w in reports.windows(2) {
            assert!(
                w[1].best >= w[0].best - 1e-12,
                "{:?} -> {:?}",
                w[0].best,
                w[1].best
            );
            assert!(w[1].hall_of_fame_best >= w[0].hall_of_fame_best);
        }
        let best = evo.best().unwrap();
        assert!(best.score.is_finite());
        assert_eq!(best.score, reports.last().unwrap().hall_of_fame_best);
        assert!(evo.hall_of_fame().len() <= HALL_OF_FAME);
        let mut sim = life_soup(12, 12);
        evo.apply_best(&mut sim).unwrap();
        assert_eq!(
            sim.get_param("rule.subrules[0].count"),
            Some(best.genome.0[0].clone())
        );
        assert!(
            evo.apply_elite(0, &mut sim).is_err(),
            "no archive in objective mode"
        );
        assert!(evo.archive().is_none() && evo.archive_snapshot().is_none());
        assert!(format!("{evo:?}").contains("Objective"));
        // evaluate() agrees with what the generation loop stored.
        let g0 = &evo.reports()[0];
        assert_eq!(
            evo.evaluate(&g0.best_genome, 0, 0),
            evo.evaluate(&g0.best_genome, 0, 0)
        );
    }

    #[test]
    fn runs_are_identical_across_thread_counts() {
        let cfg = small(EvolveConfig {
            genes: life_genes(),
            objective: Some(fraction_target(0.3)),
            ..EvolveConfig::default()
        });
        let run = |threads: usize| {
            set_thread_override(threads);
            let mut evo = Evolution::new(life_soup(16, 16), &cfg).unwrap();
            let r = evo.run(3, |_| {});
            clear_thread_override();
            r
        };
        assert_eq!(run(1), run(4));
    }

    #[test]
    fn density_classification_draws_its_own_starts_and_scores_in_unit_range() {
        let cfg = EvolveConfig {
            population: 6,
            steps: 40,
            repeats: 4,
            genes: vec![GeneSpec::new("rule.subrules[*].wolfram_code")],
            objective: Some(Objective::maximise(Metric::DensityClassification {
                types: ["X".into(), "Inactive".into()],
            })),
            ..EvolveConfig::default()
        };
        let mut evo = Evolution::new(row(21), &cfg).unwrap();
        let r = evo.step_generation();
        assert!((0.0..=1.0).contains(&r.best), "{}", r.best);
        for ind in evo.reports()[0].best_named.values() {
            assert!(matches!(ind, ParamValue::Bits(v) if *v < 256));
        }
        // The start is random: two seeds of the evaluation differ from the template.
        let cells = evo.initial_cells(mix(cfg.seed ^ 0x1C)).unwrap();
        assert_eq!(cells.len(), 21);
        assert_ne!(cells, evo.template.cells().to_vec());
        let again = evo.initial_cells(mix(cfg.seed ^ 0x1C)).unwrap();
        assert_eq!(cells, again, "the same generation draws the same start");
    }

    #[test]
    fn random_initial_conditions_are_shared_within_a_generation() {
        let cfg = small(EvolveConfig {
            genes: life_genes(),
            objective: Some(Objective::maximise(Metric::Fraction {
                types: vec!["Alive".into()],
            })),
            initial: InitialCondition::Random {
                types: vec!["Alive".into(), "Inactive".into()],
                weights: vec![0.3, 0.7],
            },
            ..EvolveConfig::default()
        });
        let evo = Evolution::new(life_soup(10, 10), &cfg).unwrap();
        let cells = evo.initial_cells(mix(0x1C)).unwrap();
        let alive = cells
            .iter()
            .filter(|c| **c == CellType::from("Alive"))
            .count();
        assert!((15..=45).contains(&alive), "about 30 of 100 alive: {alive}");
        let g = evo.population()[0].genome.clone();
        assert_eq!(evo.evaluate(&g, 0, 0), evo.evaluate(&g, 0, 0));
        // Equal weights when none are given.
        let cfg2 = EvolveConfig {
            initial: InitialCondition::Random {
                types: vec!["Alive".into(), "Inactive".into()],
                weights: vec![],
            },
            ..cfg.clone()
        };
        let evo2 = Evolution::new(life_soup(10, 10), &cfg2).unwrap();
        let cells = evo2.initial_cells(7).unwrap();
        let alive = cells
            .iter()
            .filter(|c| **c == CellType::from("Alive"))
            .count();
        assert!((30..=70).contains(&alive), "about half alive: {alive}");
    }

    #[test]
    fn a_refused_genome_scores_minus_infinity_and_is_counted_not_fatal() {
        // A gt limit gene that can fall below its count of 2 is refused by the rule.
        let cfg = small(EvolveConfig {
            genes: vec![GeneSpec::range("rule.subrules[1].limit", 0.0, 8.0)],
            objective: Some(Objective::maximise(Metric::Fraction {
                types: vec!["Alive".into()],
            })),
            immigrants: 0.5,
            ..EvolveConfig::default()
        });
        let mut evo = Evolution::new(life_soup(10, 10), &cfg).unwrap();
        let mut saw_invalid = false;
        for _ in 0..4 {
            let r = evo.step_generation();
            if r.invalid > 0 {
                saw_invalid = true;
            }
            assert!(r.best.is_finite() || r.evaluations == r.invalid);
        }
        assert!(
            saw_invalid,
            "with a 0..8 limit some draws sit below count 2"
        );
        assert!(
            evo.population().iter().all(|i| i.fitness.is_nan()),
            "the next generation is unevaluated"
        );
        let bad = Genome(vec![ParamValue::Int(1)]);
        assert_eq!(evo.evaluate(&bad, 0, 0), f64::NEG_INFINITY);
        // A population with no valid genome at all just re-samples.
        let cfg2 = EvolveConfig {
            genes: vec![GeneSpec::range("rule.subrules[1].limit", 0.0, 1.0)],
            ..cfg
        };
        let mut evo2 = Evolution::new(life_soup(6, 6), &cfg2).unwrap();
        let r = evo2.step_generation();
        assert_eq!(r.invalid, r.evaluations);
        assert_eq!(r.best, f64::NEG_INFINITY);
        assert_eq!(evo2.population().len(), 8);
    }

    #[test]
    fn boltzmann_selection_and_zero_crossover_run() {
        let cfg = small(EvolveConfig {
            genes: life_genes(),
            objective: Some(fraction_target(0.2)),
            selection: Selection::Boltzmann { beta: 5.0 },
            crossover: 0.0,
            mutation: 1.0,
            elite: 0,
            ..EvolveConfig::default()
        });
        let mut evo = Evolution::new(life_soup(8, 8), &cfg).unwrap();
        let reports = evo.run(3, |_| {});
        assert_eq!(reports.len(), 3);
        assert!(reports.iter().all(|r| r.best.is_finite()));
    }

    #[test]
    fn novelty_search_grows_an_archive_and_adapts_its_threshold() {
        let cfg = EvolveConfig {
            population: 10,
            steps: 24,
            repeats: 1,
            genes: vec![GeneSpec::new("rule.subrules[*].wolfram_code")],
            objective: Some(Objective::maximise(Metric::Fraction {
                types: vec!["X".into()],
            })),
            search: Search::Novelty {
                k: 3,
                threshold: None,
            },
            descriptors: vec![
                DescriptorSpec::new(Metric::Fraction {
                    types: vec!["X".into()],
                }),
                DescriptorSpec {
                    when: When::Mean,
                    ..DescriptorSpec::new(Metric::Activity)
                },
                DescriptorSpec::new(Metric::Period { window: 16 }),
            ],
            ..EvolveConfig::default()
        };
        let mut evo = Evolution::new(row(31), &cfg).unwrap();
        let t0 = evo.novelty_threshold().unwrap();
        assert!((t0 - 0.1 * 3f64.sqrt()).abs() < 1e-12);
        assert!(evo.novelty_archive().is_empty());
        let reports = evo.run(4, |_| {});
        let last = reports.last().unwrap();
        assert!(last.novelty_mean.is_some());
        assert!(
            last.archive_size.unwrap() >= 1,
            "something was novel enough"
        );
        assert!(last.archive.is_none());
        let t1 = evo.novelty_threshold().unwrap();
        assert!(t1 > 0.0 && t1 != t0, "the threshold adapted: {t0} -> {t1}");
        let arch = evo.novelty_archive();
        assert_eq!(arch.len(), last.archive_size.unwrap());
        assert!(arch.iter().all(|(d, g)| d.len() == 3 && g.len() == 1));
        assert!(
            Evolution::new(
                life_soup(6, 6),
                &small(EvolveConfig {
                    genes: life_genes(),
                    objective: Some(fraction_target(0.3)),
                    ..EvolveConfig::default()
                })
            )
            .unwrap()
            .novelty_threshold()
            .is_none()
        );
        // Fitness used for breeding is novelty (finite, >= 0), score is the objective.
        assert!(evo.hall_of_fame().iter().all(|i| i.score.is_finite()));
        assert_eq!(evo.descriptor().unwrap().dims(), 3);
        assert!(
            evo.population()[0].thumbnail.is_none(),
            "unevaluated children have no picture yet"
        );
        // A fixed threshold and reservoir cap are honoured.
        let cfg2 = EvolveConfig {
            search: Search::Novelty {
                k: 2,
                threshold: Some(1e-9),
            },
            ..cfg.clone()
        };
        let mut evo2 = Evolution::new(row(31), &cfg2).unwrap();
        evo2.novelty.as_mut().unwrap().cap = 5;
        evo2.run(3, |_| {});
        assert!(evo2.novelty.as_ref().unwrap().points.len() <= 5);
        assert!(evo2.novelty.as_ref().unwrap().seen > 5);
        let run = |threads: usize| {
            set_thread_override(threads);
            let mut e = Evolution::new(row(31), &cfg).unwrap();
            let r = e.run(2, |_| {});
            clear_thread_override();
            r
        };
        assert_eq!(run(1), run(4));
    }

    #[test]
    fn map_elites_fills_an_archive_and_can_load_an_elite() {
        let cfg = EvolveConfig {
            steps: 12,
            repeats: 1,
            genes: life_genes(),
            objective: Some(Objective::maximise(Metric::Lifetime)),
            search: Search::MapElites {
                batch: 12,
                iso_line: true,
            },
            descriptors: vec![
                DescriptorSpec {
                    when: When::Mean,
                    bins: 6,
                    ..DescriptorSpec::new(Metric::Activity)
                },
                DescriptorSpec {
                    bins: 4,
                    ..DescriptorSpec::new(Metric::Entropy)
                },
            ],
            ..EvolveConfig::default()
        };
        let mut evo = Evolution::new(life_soup(12, 12), &cfg).unwrap();
        assert_eq!(
            evo.population().len(),
            12,
            "the first batch is drawn from the gene ranges"
        );
        let reports = evo.run(5, |_| {});
        let mut last_cov = 0.0;
        for r in &reports {
            let s = r.archive.as_ref().expect("archive stats");
            assert!(s.coverage >= last_cov, "coverage never shrinks");
            last_cov = s.coverage;
            assert!(s.qd_score >= 0.0);
            assert_eq!(r.evaluations, 12);
        }
        let archive = evo.archive().unwrap();
        let stats = archive.stats();
        assert!(stats.elites >= 2, "{stats:?}");
        assert_eq!(archive.len(), 24);
        for (_, e) in archive.elites() {
            assert!(e.thumbnail.is_some(), "elites carry a picture");
            let t = e.thumbnail.as_ref().unwrap();
            assert_eq!((t.width, t.height), (12, 12));
            // Children stay inside the gene ranges after iso+line variation.
            for (gene, v) in evo.space().genes().iter().zip(&e.genome.0) {
                if let (GeneKind::Int { lo, hi }, ParamValue::Int(x)) = (&gene.kind, v) {
                    assert!((lo..=hi).contains(&x));
                }
            }
        }
        let snap = evo.archive_snapshot().unwrap();
        assert_eq!(snap.dims, vec![6, 4]);
        assert_eq!(snap.cells.len(), 24);
        assert_eq!(snap.stats, stats);
        let (cell, elite) = archive.best().unwrap();
        let mut sim = life_soup(12, 12);
        evo.apply_elite(cell, &mut sim).unwrap();
        assert_eq!(
            sim.get_param("rule.subrules[0].count"),
            Some(elite.genome.0[0].clone())
        );
        let empty = (0..24).find(|i| archive.get(*i).is_none());
        if let Some(i) = empty {
            assert!(evo.apply_elite(i, &mut sim).is_err());
        }
        assert!(evo.apply_elite(999, &mut sim).is_err());
        assert!(evo.best().is_some());
        // Illumination: no objective means every genome scores 1 and QD == elites.
        let cfg2 = EvolveConfig {
            objective: None,
            search: Search::MapElites {
                batch: 8,
                iso_line: false,
            },
            ..cfg.clone()
        };
        let mut evo2 = Evolution::new(life_soup(12, 12), &cfg2).unwrap();
        let r = evo2.run(3, |_| {}).pop().unwrap();
        let s = r.archive.unwrap();
        assert_eq!(s.qd_score, s.elites as f64);
        assert_eq!(s.obj_max, 1.0);
        // Thumbnails can be switched off; 1D archives get space-time strips.
        let cfg3 = EvolveConfig {
            thumbnails: false,
            genes: vec![GeneSpec::new("rule.subrules[*].wolfram_code")],
            descriptors: vec![DescriptorSpec {
                bins: 8,
                ..DescriptorSpec::new(Metric::Fraction {
                    types: vec!["X".into()],
                })
            }],
            objective: None,
            ..cfg2.clone()
        };
        let mut evo3 = Evolution::new(row(31), &cfg3).unwrap();
        evo3.run(2, |_| {});
        assert!(
            evo3.archive()
                .unwrap()
                .elites()
                .all(|(_, e)| e.thumbnail.is_none())
        );
        let cfg4 = EvolveConfig {
            thumbnails: true,
            ..cfg3
        };
        let mut evo4 = Evolution::new(row(31), &cfg4).unwrap();
        evo4.run(2, |_| {});
        let (_, e) = evo4.archive().unwrap().elites().next().unwrap();
        let t = e.thumbnail.as_ref().unwrap();
        assert_eq!(t.width, 31);
        assert_eq!(
            t.height as u64,
            1 + cfg4.steps,
            "one row per step plus the start"
        );
    }

    #[test]
    fn config_is_validated_and_round_trips_json() {
        let base = || {
            small(EvolveConfig {
                genes: life_genes(),
                objective: Some(fraction_target(0.3)),
                ..EvolveConfig::default()
            })
        };
        let s = life_soup(6, 6);
        let cases: Vec<(EvolveConfig, &str)> = vec![
            (
                EvolveConfig {
                    population: 1,
                    ..base()
                },
                "population",
            ),
            (EvolveConfig { elite: 8, ..base() }, "elite"),
            (EvolveConfig { steps: 0, ..base() }, "steps"),
            (
                EvolveConfig {
                    repeats: 0,
                    ..base()
                },
                "repeats",
            ),
            (
                EvolveConfig {
                    crossover: 1.5,
                    ..base()
                },
                "crossover",
            ),
            (
                EvolveConfig {
                    mutation: -0.1,
                    ..base()
                },
                "mutation",
            ),
            (
                EvolveConfig {
                    sigma: 0.0,
                    ..base()
                },
                "sigma",
            ),
            (
                EvolveConfig {
                    immigrants: 2.0,
                    ..base()
                },
                "immigrants",
            ),
            (
                EvolveConfig {
                    selection: Selection::Tournament { k: 0 },
                    ..base()
                },
                "tournament",
            ),
            (
                EvolveConfig {
                    selection: Selection::Boltzmann { beta: f64::NAN },
                    ..base()
                },
                "beta",
            ),
            (
                EvolveConfig {
                    objective: None,
                    ..base()
                },
                "objective is required",
            ),
            (
                EvolveConfig {
                    search: Search::Novelty {
                        k: 15,
                        threshold: None,
                    },
                    ..base()
                },
                "descriptors",
            ),
            (
                EvolveConfig {
                    search: Search::Novelty {
                        k: 0,
                        threshold: None,
                    },
                    descriptors: vec![DescriptorSpec::new(Metric::Activity)],
                    ..base()
                },
                "novelty k",
            ),
            (
                EvolveConfig {
                    search: Search::Novelty {
                        k: 3,
                        threshold: Some(-1.0),
                    },
                    descriptors: vec![DescriptorSpec::new(Metric::Activity)],
                    ..base()
                },
                "threshold",
            ),
            (
                EvolveConfig {
                    search: Search::MapElites {
                        batch: 0,
                        iso_line: false,
                    },
                    descriptors: vec![DescriptorSpec::new(Metric::Activity)],
                    ..base()
                },
                "batch",
            ),
            (
                EvolveConfig {
                    descriptors: vec![DescriptorSpec::new(Metric::Lifetime); 4],
                    ..base()
                },
                "1 to 3 axes",
            ),
            (
                EvolveConfig {
                    descriptors: vec![DescriptorSpec::new(Metric::Series {
                        types: vec![],
                        target: vec![1.0],
                    })],
                    ..base()
                },
                "cannot be an axis",
            ),
            (
                EvolveConfig {
                    objective: Some(Objective::maximise(Metric::Fraction {
                        types: vec!["Ghost".into()],
                    })),
                    ..base()
                },
                "Ghost",
            ),
            (
                EvolveConfig {
                    genes: vec![GeneSpec::new("model.p0")],
                    ..base()
                },
                "model.p0",
            ),
            (
                EvolveConfig {
                    initial: InitialCondition::Random {
                        types: vec![],
                        weights: vec![],
                    },
                    ..base()
                },
                "at least one type",
            ),
            (
                EvolveConfig {
                    initial: InitialCondition::Random {
                        types: vec!["Ghost".into()],
                        weights: vec![],
                    },
                    ..base()
                },
                "Ghost",
            ),
            (
                EvolveConfig {
                    initial: InitialCondition::Random {
                        types: vec!["Alive".into()],
                        weights: vec![1.0, 2.0],
                    },
                    ..base()
                },
                "one weight per type",
            ),
            (
                EvolveConfig {
                    initial: InitialCondition::Random {
                        types: vec!["Alive".into()],
                        weights: vec![0.0],
                    },
                    ..base()
                },
                "not all zero",
            ),
        ];
        for (cfg, expect) in cases {
            let err = Evolution::new(s.clone(), &cfg).unwrap_err();
            assert!(format!("{err}").contains(expect), "{err} (wanted {expect})");
        }
        // Descriptors are allowed (and resolved) in objective mode too.
        let ok = Evolution::new(
            s.clone(),
            &EvolveConfig {
                descriptors: vec![DescriptorSpec::new(Metric::Activity)],
                ..base()
            },
        )
        .unwrap();
        assert_eq!(ok.descriptor().unwrap().dims(), 1);

        let json = r#"{
            "population": 12, "generations": 5, "steps": 20,
            "genes": [{"key": "rule.subrules[0].count", "range": [3, 8]}],
            "objective": {"metric": "fraction", "types": ["Alive"], "goal": {"target": 0.3}},
            "search": {"map_elites": {"batch": 16, "iso_line": true}},
            "descriptors": [{"metric": "activity", "when": "mean", "bins": 10}, {"metric": "entropy"}],
            "selection": {"boltzmann": {"beta": 4.0}},
            "initial": {"random": {"types": ["Alive", "Inactive"], "weights": [0.3, 0.7]}}
        }"#;
        let cfg: EvolveConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.population, 12);
        assert_eq!(
            cfg.search,
            Search::MapElites {
                batch: 16,
                iso_line: true
            }
        );
        assert_eq!(cfg.descriptors[1].bins, 20);
        assert_eq!(cfg.selection, Selection::Boltzmann { beta: 4.0 });
        assert!(matches!(cfg.initial, InitialCondition::Random { .. }));
        assert_eq!(cfg.repeats, 3, "default");
        let back: EvolveConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.search, cfg.search);
        assert_eq!(back.descriptors, cfg.descriptors);
        let minimal: EvolveConfig =
            serde_json::from_str(r#"{"objective": {"metric": "lifetime"}}"#).unwrap();
        assert_eq!(minimal.search, Search::Objective);
        assert_eq!(minimal.initial, InitialCondition::Fixed);
        assert_eq!(minimal.selection, Selection::Tournament { k: 3 });
        let novelty: EvolveConfig = serde_json::from_str(r#"{"search": {"novelty": {}}}"#).unwrap();
        assert_eq!(
            novelty.search,
            Search::Novelty {
                k: 15,
                threshold: None
            }
        );
        let plain: EvolveConfig =
            serde_json::from_str(r#"{"search": "objective", "initial": "fixed"}"#).unwrap();
        assert_eq!(plain.search, Search::Objective);
        assert!(serde_json::from_str::<EvolveConfig>(r#"{"prior": {}}"#).is_err());
        assert!(serde_json::from_str::<EvolveConfig>(r#"{"search": "nonsense"}"#).is_err());
        let report_json = serde_json::to_string(&GenerationReport {
            generation: 1,
            best: 0.5,
            mean: 0.2,
            sd: 0.1,
            min: 0.0,
            best_value: 0.5,
            best_genome: Genome(vec![ParamValue::Int(3)]),
            best_named: BTreeMap::new(),
            evaluations: 8,
            invalid: 0,
            hall_of_fame_best: 0.5,
            archive: None,
            novelty_mean: None,
            archive_size: None,
        })
        .unwrap();
        assert!(
            !report_json.contains("archive"),
            "absent optionals are skipped: {report_json}"
        );
    }

    #[test]
    fn a_custom_fitness_can_replace_the_objective() {
        struct Dead;
        impl Fitness for Dead {
            fn sample(&self, sim: &Sim) -> f64 {
                super::super::metrics::fraction(sim, &[CellType::from("Alive")])
            }
            fn aggregate(&self, samples: &[f64]) -> f64 {
                samples[samples.len() - 1]
            }
            fn score(&self, samples: &[f64]) -> f64 {
                -self.aggregate(samples)
            }
        }
        let cfg = small(EvolveConfig {
            genes: life_genes(),
            ..EvolveConfig::default()
        });
        let mut evo = Evolution::with_fitness(life_soup(8, 8), &cfg, Arc::new(Dead)).unwrap();
        let r = evo.step_generation();
        assert!(r.best <= 0.0 && r.best_value >= 0.0);
    }
}
