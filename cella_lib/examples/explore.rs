//! Run the `ensemble` or `evolve` block of any config from the command line.
//!
//! Background: an *ensemble* is a population of simulations ("members"), each
//! with its own gene values, that is stepped forward together and can learn
//! from an observed mask. *Evolve* runs a genetic search (population,
//! generations, scoring) over the same genes to find good settings.
//!
//! ```text
//! cd cella_lib
//! cargo run --release --example explore -- <config.json> ensemble <steps> [report.json]
//! cargo run --release --example explore -- <config.json> evolve [report.json] [--cell i[,j[,k]]]
//! ```
//!
//! The report path defaults to `explore_ensemble.json` / `explore_evolve.json`
//! in the current directory. `--cell` picks one cell of the quality-diversity
//! archive (the grid of "elite" genomes) instead of the overall best genome;
//! it only has an effect when the evolve block uses an archive. The report
//! path can go before or after `--cell i,j`.
//!
//! Environment overrides (all optional):
//! - `EXPLORE_SEED`: replace the seed of the grid and of the ensemble/evolve block
//! - `EXPLORE_GENES=path`: a JSON array of genes replacing the block's list
//! - `EXPLORE_EVERY`: ensemble, report every N steps (default 10)
//! - `EXPLORE_MASKS=path`: ensemble, `[{"step": k, "mask": [0/1 per cell]}]`;
//!   at each listed step the forecast is scored against the mask *before*
//!   the ensemble learns from it
//! - `EXPLORE_GENERATIONS`: evolve, how many generations to run
//! - `EXPLORE_THUMBS=1`: evolve, include archive thumbnails in the report
//! - `EXPLORE_APPLY=path`: evolve, write the config with the best genome
//!   (or the `--cell` elite) applied
//!
//! Progress goes to stderr; the report is JSON.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cella_lib::config::{CellaConfig, Config1D, Config2D};
use cella_lib::explore::metrics::{activity, brier, entropy, fraction, iou};
use cella_lib::explore::{ArchiveReport, GeneSpec, GenerationReport, Sim};
use cella_lib::{CellType, ParamValue};
use serde::{Deserialize, Serialize};

fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

#[derive(Deserialize)]
struct MaskAt {
    step: u64,
    mask: Vec<u8>,
}

#[derive(Serialize)]
struct Stat {
    mean: f64,
    sd: f64,
}

#[derive(Serialize)]
struct GeneStat {
    mean: f64,
    sd: f64,
    min: f64,
    max: f64,
}

#[derive(Serialize)]
struct Assimilation {
    consensus_iou: f64,
    mean_member_iou: f64,
    best_member_iou: f64,
    brier: f64,
    ess: f64,
    immigrants: usize,
    rejected: usize,
}

#[derive(Serialize)]
struct Batch {
    step: u64,
    tracked: BTreeMap<String, Stat>,
    consensus_cells: usize,
    metrics: BTreeMap<String, Stat>,
    genome: BTreeMap<String, GeneStat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assimilation: Option<Assimilation>,
}

#[derive(Serialize)]
struct EnsembleReport {
    mode: String,
    config: String,
    members: usize,
    seed: u64,
    beta: f64,
    sigma: f64,
    immigrants: f64,
    genes: Vec<GeneSpec>,
    track: Vec<String>,
    batches: Vec<Batch>,
}

#[derive(Serialize)]
struct HallEntry {
    score: f64,
    value: f64,
    genome: BTreeMap<String, ParamValue>,
}

#[derive(Serialize)]
struct NoveltyReport {
    threshold: f64,
    archive: Vec<(Vec<f64>, BTreeMap<String, ParamValue>)>,
}

#[derive(Serialize)]
struct EvolveReport {
    mode: String,
    config: String,
    population: usize,
    generations: usize,
    repeats: usize,
    steps: u64,
    seed: u64,
    search: cella_lib::explore::Search,
    #[serde(skip_serializing_if = "Option::is_none")]
    objective: Option<cella_lib::Objective>,
    generations_log: Vec<GenerationReport>,
    hall_of_fame: Vec<HallEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive: Option<ArchiveReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    novelty: Option<NoveltyReport>,
    best_config: CellaConfig,
}

/// The config with its grid replaced by `sim`'s current cells, rule, model
/// and seed (blocks and colours kept), so a tuned grid can be saved.
fn config_with_sim(cfg: &CellaConfig, sim: &Sim) -> CellaConfig {
    let names = |cells: &[CellType]| cells.iter().map(|c| c.as_str().to_string()).collect();
    match (cfg, sim) {
        (CellaConfig::D1(c), Sim::D1(g)) => CellaConfig::D1(Config1D {
            width: g.width,
            history_limit: g.history_limit,
            initial: names(g.cells()),
            rule: g.rule.clone(),
            seed: g.seed,
            ..c.clone()
        }),
        (CellaConfig::D2(c), Sim::D2(g)) => CellaConfig::D2(Config2D {
            width: g.width,
            height: g.height,
            history_limit: g.history_limit,
            initial: names(g.cells()),
            rule: g.rule.clone(),
            model: g.model.clone(),
            seed: g.seed,
            ..c.clone()
        }),
        _ => cfg.clone(),
    }
}

fn apply_overrides(cfg: &mut CellaConfig) {
    let seed = env_u64("EXPLORE_SEED");
    let genes: Option<Vec<GeneSpec>> = std::env::var("EXPLORE_GENES")
        .ok()
        .map(|p| load(Path::new(&p)));
    let generations = env_u64("EXPLORE_GENERATIONS").map(|g| g as usize);
    let (ens, evo, grid_seed) = match cfg {
        CellaConfig::D1(c) => (&mut c.ensemble, &mut c.evolve, &mut c.seed),
        CellaConfig::D2(c) => (&mut c.ensemble, &mut c.evolve, &mut c.seed),
    };
    if let Some(s) = seed {
        *grid_seed = s;
        if let Some(e) = ens.as_mut() {
            e.seed = s;
        }
        if let Some(e) = evo.as_mut() {
            e.seed = s;
        }
    }
    if let Some(g) = genes {
        if let Some(e) = ens.as_mut() {
            e.genes = g.clone();
        }
        if let Some(e) = evo.as_mut() {
            e.genes = g;
        }
    }
    if let (Some(n), Some(e)) = (generations, evo.as_mut()) {
        e.generations = n;
    }
}

fn write_json(path: &Path, value: &impl Serialize) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn run_ensemble(cfg: &CellaConfig, config_path: &str, steps: u64, out: &Path) {
    let ens_cfg = cfg
        .ensemble()
        .expect("the config has no \"ensemble\" block")
        .clone();
    let mut ens = cfg
        .build_ensemble()
        .expect("the grid builds")
        .unwrap_or_else(|e| panic!("ensemble block rejected: {e}"));
    let every = env_u64("EXPLORE_EVERY").unwrap_or(10).max(1);
    let masks: Vec<MaskAt> = std::env::var("EXPLORE_MASKS")
        .ok()
        .map(|p| load(Path::new(&p)))
        .unwrap_or_default();
    let track: Vec<CellType> = ens.track().to_vec();
    let gene_keys: Vec<String> = ens.space().genes().iter().map(|g| g.key.clone()).collect();
    eprintln!(
        "{config_path}: {} members, {} genes, tracking {:?}, {steps} steps",
        ens.len(),
        gene_keys.len(),
        track.iter().map(|t| t.as_str()).collect::<Vec<_>>()
    );
    let mut batches = Vec::new();
    let mut done = 0u64;
    while done < steps {
        let next_every = (done / every + 1) * every;
        let next_mask = masks
            .iter()
            .map(|m| m.step)
            .filter(|s| *s > done)
            .min()
            .unwrap_or(u64::MAX);
        let target = next_every.min(next_mask).min(steps);
        ens.step_n(target - done).expect("members step");
        done = target;
        let mut tracked = BTreeMap::new();
        for t in &track {
            let (mean, sd) = ens.metric_stats(|s| fraction(s, std::slice::from_ref(t)));
            tracked.insert(t.as_str().to_string(), Stat { mean, sd });
        }
        let mut metrics = BTreeMap::new();
        let (m, s) = ens.metric_stats(activity);
        metrics.insert("activity".to_string(), Stat { mean: m, sd: s });
        let (m, s) = ens.metric_stats(entropy);
        metrics.insert("entropy".to_string(), Stat { mean: m, sd: s });
        let mut genome = BTreeMap::new();
        for k in &gene_keys {
            if let Some((mean, sd, min, max)) = ens.genome_stats(k) {
                genome.insert(k.clone(), GeneStat { mean, sd, min, max });
            }
        }
        let consensus_cells = ens.consensus(&track, 0.5).iter().filter(|b| **b).count();
        let assimilation = masks.iter().find(|m| m.step == done).map(|m| {
            let observed: Vec<bool> = m.mask.iter().map(|v| *v != 0).collect();
            assert_eq!(observed.len(), ens.members()[0].sim.len(), "mask length");
            let prob = ens.state_probability(&track);
            let consensus: Vec<bool> = prob.iter().map(|p| *p >= 0.5).collect();
            let ious: Vec<f64> = (0..ens.len())
                .map(|i| iou(&ens.member_mask(i, &track), &observed))
                .collect();
            let report = ens
                .assimilate(&observed, &track)
                .expect("mask matches the grid");
            Assimilation {
                consensus_iou: iou(&consensus, &observed),
                mean_member_iou: ious.iter().sum::<f64>() / ious.len() as f64,
                best_member_iou: ious.iter().cloned().fold(0.0, f64::max),
                brier: brier(&prob, &observed),
                ess: report.effective_sample_size,
                immigrants: report.immigrants,
                rejected: report.rejected,
            }
        });
        eprintln!(
            "  step {done:6} | consensus cells {consensus_cells:7} | {}",
            tracked
                .iter()
                .map(|(k, v)| format!("{k} {:.3}±{:.3}", v.mean, v.sd))
                .collect::<Vec<_>>()
                .join(" ")
        );
        if let Some(a) = &assimilation {
            eprintln!(
                "         learned: consensus IoU {:.3} | member IoU mean {:.3} best {:.3} | Brier {:.4} | ESS {:.1} | immigrants {}",
                a.consensus_iou, a.mean_member_iou, a.best_member_iou, a.brier, a.ess, a.immigrants
            );
        }
        batches.push(Batch {
            step: done,
            tracked,
            consensus_cells,
            metrics,
            genome,
            assimilation,
        });
    }
    let report = EnsembleReport {
        mode: "ensemble".into(),
        config: config_path.to_string(),
        members: ens_cfg.members,
        seed: ens_cfg.seed,
        beta: ens_cfg.beta,
        sigma: ens_cfg.sigma,
        immigrants: ens_cfg.immigrants,
        genes: ens_cfg.genes.clone(),
        track: track.iter().map(|t| t.as_str().to_string()).collect(),
        batches,
    };
    write_json(out, &report);
    eprintln!("-> {}", out.display());
}

/// The positional arguments (the ones that are not flags) in `rest`, which is
/// everything after the mode. Today the only flag that takes a value is
/// `--cell`; its value is skipped together with it, so `--cell 1,2` never
/// leaks `1,2` into the positionals. Any other `--flag` is skipped alone.
fn positionals(rest: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut iter = rest.iter();
    while let Some(a) = iter.next() {
        if a == "--cell" {
            iter.next(); // its value belongs to the flag
        } else if !a.starts_with("--") {
            out.push(a.as_str());
        }
    }
    out
}

fn parse_cell(args: &[String]) -> Option<Vec<u32>> {
    let pos = args.iter().position(|a| a == "--cell")?;
    let spec = args.get(pos + 1)?;
    spec.split(',').map(|s| s.trim().parse().ok()).collect()
}

fn run_evolve(cfg: &CellaConfig, config_path: &str, out: &Path, cell: Option<Vec<u32>>) {
    let evo_cfg = cfg
        .evolve()
        .expect("the config has no \"evolve\" block")
        .clone();
    let mut evo = cfg
        .build_evolution()
        .expect("the grid builds")
        .unwrap_or_else(|e| panic!("evolve block rejected: {e}"));
    eprintln!(
        "{config_path}: population {}, {} generations, {} steps x {} repeats, search {:?}",
        evo_cfg.population, evo_cfg.generations, evo_cfg.steps, evo_cfg.repeats, evo_cfg.search
    );
    let start = std::time::Instant::now();
    let log = evo.run(evo_cfg.generations, |r| {
        let extra = match (&r.archive, r.novelty_mean) {
            (Some(a), _) => format!(
                " | archive {}/{} coverage {:.2} QD {:.2}",
                a.elites,
                (a.elites as f64 / a.coverage.max(1e-12)).round(),
                a.coverage,
                a.qd_score
            ),
            (None, Some(n)) => format!(
                " | novelty mean {:.3} archive {}",
                n,
                r.archive_size.unwrap_or(0)
            ),
            _ => String::new(),
        };
        eprintln!(
            "  gen {:3} | best {:.4} mean {:.4} sd {:.4} | value {:.4} | invalid {}{} | {:.1}s",
            r.generation,
            r.best,
            r.mean,
            r.sd,
            r.best_value,
            r.invalid,
            extra,
            start.elapsed().as_secs_f64()
        );
    });
    let thumbs = env_u64("EXPLORE_THUMBS").unwrap_or(0) > 0;
    let mut tuned = cfg.build_sim().expect("the grid builds");
    let applied = match (&cell, evo.archive()) {
        (Some(coords), Some(archive)) => {
            let mut index = 0usize;
            for (c, b) in coords.iter().zip(&archive.dims) {
                index = index * (*b as usize) + (*c as usize).min(*b as usize - 1);
            }
            evo.apply_elite(index, &mut tuned)
                .map(|_| format!("archive cell {coords:?}"))
        }
        _ => evo
            .apply_best(&mut tuned)
            .map(|_| "best genome".to_string()),
    };
    match &applied {
        Ok(what) => eprintln!("applied the {what} to the grid"),
        Err(e) => eprintln!("nothing applied: {e}"),
    }
    let best_config = config_with_sim(cfg, &tuned);
    if let Ok(path) = std::env::var("EXPLORE_APPLY") {
        write_json(Path::new(&path), &best_config);
        eprintln!("-> {path}");
    }
    let report = EvolveReport {
        mode: "evolve".into(),
        config: config_path.to_string(),
        population: evo_cfg.population,
        generations: evo_cfg.generations,
        repeats: evo_cfg.repeats,
        steps: evo_cfg.steps,
        seed: evo_cfg.seed,
        search: evo_cfg.search.clone(),
        objective: evo_cfg.objective.clone(),
        generations_log: log,
        hall_of_fame: evo
            .hall_of_fame()
            .iter()
            .map(|i| HallEntry {
                score: i.score,
                value: i.value,
                genome: evo.named(&i.genome),
            })
            .collect(),
        archive: evo.archive().map(|a| a.to_report(thumbs)),
        novelty: evo.novelty_threshold().map(|threshold| NoveltyReport {
            threshold,
            archive: evo.novelty_archive(),
        }),
        best_config,
    };
    write_json(out, &report);
    eprintln!("-> {}", out.display());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: explore <config.json> ensemble <steps> [report.json] | explore <config.json> evolve [report.json] [--cell i,j]";
    let config_path = args.get(1).expect(usage).clone();
    let mode = args.get(2).expect(usage).clone();
    let mut cfg: CellaConfig = load(Path::new(&config_path));
    apply_overrides(&mut cfg);
    let positional = positionals(&args[3..]);
    match mode.as_str() {
        "ensemble" => {
            let steps: u64 = positional
                .first()
                .and_then(|s| s.parse().ok())
                .expect("ensemble mode needs <steps>");
            let out = PathBuf::from(
                positional
                    .get(1)
                    .copied()
                    .unwrap_or("explore_ensemble.json"),
            );
            run_ensemble(&cfg, &config_path, steps, &out);
        }
        "evolve" => {
            let out = PathBuf::from(
                positional
                    .first()
                    .copied()
                    .unwrap_or("explore_evolve.json"),
            );
            run_evolve(&cfg, &config_path, &out, parse_cell(&args));
        }
        _ => panic!("{usage}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cell_value_is_not_a_positional() {
        let args = strings(&["--cell", "1,2"]);
        assert!(positionals(&args).is_empty());
        // The report path may come before or after the flag.
        let args = strings(&["out.json", "--cell", "1,2"]);
        assert_eq!(positionals(&args), vec!["out.json"]);
        let args = strings(&["--cell", "1,2", "out.json"]);
        assert_eq!(positionals(&args), vec!["out.json"]);
        let args = strings(&["50", "--cell", "3", "out.json"]);
        assert_eq!(positionals(&args), vec!["50", "out.json"]);
    }

    #[test]
    fn plain_arguments_are_positionals() {
        let args = strings(&["10", "r.json"]);
        assert_eq!(positionals(&args), vec!["10", "r.json"]);
    }

    #[test]
    fn parse_cell_reads_the_value_after_the_flag() {
        let args = strings(&["cfg", "evolve", "--cell", "1, 2"]);
        assert_eq!(parse_cell(&args), Some(vec![1, 2]));
        assert_eq!(parse_cell(&strings(&["cfg", "evolve"])), None);
    }
}
