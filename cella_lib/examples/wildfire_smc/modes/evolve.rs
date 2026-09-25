//! `evolve` mode (validation E36) — fit first: a genetic algorithm
//! ([`Evolution`] with the wildfire driver) searches the genes for the
//! settings whose single run best matches the first `SMC_FIT_DAYS` observed
//! perimeters (mean IoU over those days). `fit_first_days` is that fit; the
//! winner is then run forward as an `open` ensemble by
//! [`super::open::run`] — `main` calls the two in sequence, exactly like
//! `open`/`assim` call [`super::open::run`] directly.

use std::collections::BTreeMap;
use std::sync::Arc;

use cella_lib::explore::Sim;
use cella_lib::explore::metrics::{Fitness, iou};
use cella_lib::wildfire::driver::WildfireDriver;
use cella_lib::{CellType, Evolution, EvolveConfig, GeneSpec, ParamValue};
use serde::Serialize;

use crate::knobs::env_usize;
use crate::{Scenario, Truth, mask_at, weather_schedule};

/// The offline fit of `evolve` mode.
#[derive(Serialize)]
pub(crate) struct FitReport {
    pub(crate) fit_days: usize,
    pub(crate) fit_steps: u64,
    pub(crate) population: usize,
    pub(crate) generations: usize,
    pub(crate) repeats: usize,
    /// Mean IoU over the fit days of the best genome's evaluation runs.
    pub(crate) best_fit_iou: f64,
    pub(crate) best_genome: BTreeMap<String, ParamValue>,
    /// `(generation, best, mean)` per generation.
    pub(crate) log: Vec<(u64, f64, f64)>,
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

/// Observation steps and hours, in order, skipping the ignition (index 0).
pub(crate) fn observation_steps(sc: &Scenario, truth: &Truth) -> Vec<(u64, f64)> {
    truth
        .observed_at
        .iter()
        .skip(1)
        .map(|&h| ((h * sc.steps_per_hour).round() as u64, h))
        .collect()
}

/// `evolve` mode, step one: a GA over `genes` scored on the first
/// `SMC_FIT_DAYS` observations. Returns the fit report and the best genome.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fit_first_days(
    sc: &Scenario,
    truth: &Truth,
    cfg: &cella_lib::config::CellaConfig,
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
            ..Default::default()
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
