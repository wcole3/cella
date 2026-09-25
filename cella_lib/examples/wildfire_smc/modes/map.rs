//! `map` mode (validation E37) — MAP-Elites illumination of the spread
//! genes (`model.p0`, `model.burn_duration`, `wind_scale`): no objective,
//! two behaviour axes (growth of the burned area, elongation of its shape)
//! over `SMC_MAP_DAYS` days of the scenario's weather. The report is the
//! archive: which shapes and sizes the model can produce at all, next to
//! the observed perimeter's own growth and elongation on the same days.
//!
//! `replay` mode (validation E43 fix round 1) lives here too — it is a
//! post-hoc diagnostic *on* a `map`-mode archive, so it shares this
//! module's report types and its `Search::MapElites`/`DescriptorSpec`
//! set-up almost verbatim; see [`run_replay`]'s own doc comment.

use std::collections::BTreeMap;
use std::path::Path;

use cella_lib::config::CellaConfig;
use cella_lib::explore::archive::{ArchiveReport, ArchiveReportElite, DescriptorSpec};
use cella_lib::explore::metrics::{When, elongation, fraction, largest_component_stats};
use cella_lib::explore::{Genome, Search, Sim};
use cella_lib::wildfire::driver::WildfireDriver;
use cella_lib::{CellType, Evolution, EvolveConfig, GeneSpec, Grid2D, Metric, ParamValue, Rule2D};
use serde::{Deserialize, Serialize};

use crate::knobs::{env_f64, env_usize};
use crate::report::{provenance, write_json};
use crate::{Scenario, Truth, load, mask_at, weather_schedule};

/// A grid holding just the observed burned set, for the shape metrics.
fn observed_sim(mask: &[bool], w: usize, h: usize) -> Sim {
    let burning = CellType::new("Burning");
    let cells = mask
        .iter()
        .map(|&b| if b { burning } else { CellType::inactive() })
        .collect();
    Sim::D2(Grid2D::new(w, h, 0, cells, Rule2D { subrules: vec![] }))
}

/// The `map` mode's report: the archive plus the observed fire's own place
/// in the same descriptor space. `Deserialize` so `replay` mode (E43 fix
/// round 1) can load one of these back and re-evaluate its elites.
#[derive(Serialize, Deserialize)]
pub(crate) struct MapReport {
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

/// `map` mode: MAP-Elites over the spread genes with growth × elongation
/// axes and no objective, plus the observed fire in the same coordinates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_map(
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
            ..Default::default()
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
    let (binary_git, binary_built_utc) = provenance();
    let report = MapReport {
        scenario: sc.id.clone(),
        binary_git,
        binary_built_utc,
        days,
        steps,
        genes: genes.to_vec(),
        generations: evo_cfg.generations,
        batch,
        archive,
        observed,
        log,
    };
    write_json(out, &report);
    eprintln!(
        "archive: {} elites, coverage {:.2}; observed (hours, growth, elongation) {:?} -> {}",
        report.archive.stats.elites,
        report.archive.stats.coverage,
        report.observed,
        out.display()
    );
}

/// One seed's replay of one elite: connected-component stats on the
/// burned set it produced. See [`run_replay`].
#[derive(Serialize)]
struct ReplaySeedResult {
    seed: u64,
    total_burned: usize,
    components: usize,
    largest_component_cells: usize,
    largest_component_fraction: f64,
    whole_set_elongation: f64,
    largest_component_elongation: f64,
}

/// One replayed elite: its place in the original archive, its genome, and
/// every seed's replay of it. See [`run_replay`].
#[derive(Serialize)]
struct ReplayEliteResult {
    coords: Vec<u32>,
    /// `[growth, elongation]` as the original `map` run recorded it.
    archive_descriptor: Vec<f64>,
    genome_named: BTreeMap<String, ParamValue>,
    seeds: Vec<ReplaySeedResult>,
}

/// `replay` mode's report. See [`run_replay`].
#[derive(Serialize)]
struct ReplayReport {
    scenario: String,
    binary_git: String,
    binary_built_utc: String,
    source_archive: String,
    observed_growth_day5: f64,
    observed_elongation_day5: f64,
    top_n: usize,
    seeds_per_elite: usize,
    /// Explains why `seeds` are 0, 1, 2, ... rather than a recovered
    /// original seed; see [`run_replay`]'s own doc comment for why.
    seed_note: String,
    elites: Vec<ReplayEliteResult>,
}

/// `replay` mode (validation E43 fix round 1): a post-hoc diagnostic on a
/// `map`-mode archive, checking whether its elongation numbers reflect one
/// stretched fire or a round core plus a few cells scattered far away
/// (e.g. spot-fire embers landed well downwind of the front).
/// [`cella_lib::explore::metrics::elongation`] (what the archive itself
/// recorded, and what `45-e43-spotting-illumination.md`'s headline table
/// reads) is a second-moment measure over *every* tracked cell with no
/// connectivity distinction, so it cannot tell the two apart; this mode
/// re-evaluates the archive's own top elites and reports
/// [`largest_component_stats`], which can.
///
/// `SMC_MAP_REPLAY=<path>` (required) names the `map`-mode report to
/// replay (e.g. `exp43_spot_illuminate/Bear_2020.json`) — its own `genes`,
/// `batch`, `generations` and `steps` are reused verbatim so a replayed
/// elite runs under the exact same settings that produced the archive.
/// From that archive, elites with growth at or above the observed day-5
/// growth are ranked by elongation and the top `SMC_REPLAY_TOP` (default
/// 5) are replayed for `SMC_REPLAY_SEEDS` (default 3) seeds each — the
/// same filter and ordering `45-e43-spotting-illumination.md`'s own table
/// uses to pick "the most stretched fire at the observed size".
///
/// **This is a re-evaluation, not a reproduction.** An archive elite does
/// not record which `(generation, index, repeat)` search-time evaluation
/// first placed its genome in that cell (see
/// [`cella_lib::explore::archive::ArchiveReportElite`]), so the original
/// seed cannot be recovered from the archive file alone. Seeds 0, 1, 2,
/// ... here are fresh, independent runs of the *same stored genome*, not
/// the same stochastic realisation that earned it its place in the
/// archive — the seed spread across those three runs is itself part of
/// what gets reported.
pub(crate) fn run_replay(
    sc: &Scenario,
    cfg: &CellaConfig,
    out: &Path,
    rot: f64,
    steps_per_day: u64,
    burnt: &[CellType; 2],
) {
    let src = std::env::var("SMC_MAP_REPLAY")
        .expect("replay mode needs SMC_MAP_REPLAY=<path to a map-mode report>");
    let report: MapReport = load(Path::new(&src));
    let top_n = env_usize("SMC_REPLAY_TOP", 5);
    let seeds = env_usize("SMC_REPLAY_SEEDS", 3) as u64;

    let (obs_g, obs_el) = report
        .observed
        .last()
        .map(|&(_, g, el)| (g, el))
        .unwrap_or((0.0, 1.0));
    let mut candidates: Vec<&ArchiveReportElite> = report
        .archive
        .elites
        .iter()
        .filter(|e| e.descriptor[0] >= obs_g)
        .collect();
    candidates.sort_by(|a, b| {
        b.descriptor[1]
            .partial_cmp(&a.descriptor[1])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(top_n);

    let types: Vec<String> = burnt.iter().map(|t| t.as_str().to_string()).collect();
    let batch = report.batch.max(1);
    let evo_cfg = EvolveConfig {
        population: batch,
        generations: report.generations,
        seed: 0,
        genes: report.genes.clone(),
        objective: None,
        search: Search::MapElites {
            batch,
            iso_line: true,
        },
        descriptors: vec![
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
        steps: report.steps,
        repeats: 1,
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
    let evo = Evolution::new(template, &evo_cfg).expect("evolution builds");

    let mut elites = Vec::new();
    for elite in &candidates {
        let genome = Genome(
            report
                .genes
                .iter()
                .map(|g| {
                    elite
                        .named
                        .get(&g.key)
                        .cloned()
                        .expect("an elite names every gene it was searched with")
                })
                .collect(),
        );
        let mut seed_results = Vec::new();
        for s in 0..seeds {
            let sim = evo
                .evaluate_genome_sim(&genome, s)
                .expect("replaying a stored elite's own genome should not be refused");
            let whole = elongation(&sim, burnt);
            let stats = largest_component_stats(&sim, burnt);
            seed_results.push(ReplaySeedResult {
                seed: s,
                total_burned: stats.total,
                components: stats.components,
                largest_component_cells: stats.largest,
                largest_component_fraction: stats.largest_fraction,
                whole_set_elongation: whole,
                largest_component_elongation: stats.largest_elongation,
            });
        }
        eprintln!(
            "  coords {:?} archive descriptor {:?}: largest-component elongation {:.2}-{:.2} across {} seeds",
            elite.coords,
            elite.descriptor,
            seed_results
                .iter()
                .map(|r| r.largest_component_elongation)
                .fold(f64::INFINITY, f64::min),
            seed_results
                .iter()
                .map(|r| r.largest_component_elongation)
                .fold(f64::NEG_INFINITY, f64::max),
            seed_results.len(),
        );
        elites.push(ReplayEliteResult {
            coords: elite.coords.clone(),
            archive_descriptor: elite.descriptor.clone(),
            genome_named: elite.named.clone(),
            seeds: seed_results,
        });
    }

    let (binary_git, binary_built_utc) = provenance();
    let out_report = ReplayReport {
        scenario: sc.id.clone(),
        binary_git,
        binary_built_utc,
        source_archive: src,
        observed_growth_day5: obs_g,
        observed_elongation_day5: obs_el,
        top_n,
        seeds_per_elite: seeds as usize,
        seed_note: "the archive does not record which (generation, index, repeat) search-time \
                    evaluation produced each elite, so these seeds (0, 1, 2, ...) are fresh \
                    re-evaluations of the stored genome, not a reproduction of the original run"
            .to_string(),
        elites,
    };
    write_json(out, &out_report);
    eprintln!(
        "replay: {} elites x {} seeds -> {}",
        out_report.elites.len(),
        seeds,
        out.display()
    );
}
