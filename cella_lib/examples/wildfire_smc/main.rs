//! Ensemble forecasting for the wildfire model (validation E24 / E25 / E28),
//! built on the library's model-agnostic [`cella_lib::Ensemble`] plus the
//! [`cella_lib::wildfire::WildfireDriver`] — the same two pieces any other
//! model would use.
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
//! `evolve` mode (validation E36) — fit first: a genetic algorithm
//! ([`cella_lib::Evolution`] with the same driver) searches the genes for the
//! settings whose single run best matches the first `SMC_FIT_DAYS` observed
//! perimeters (mean IoU over those days). The winner is then run forward as
//! an `open` ensemble (every member = the fitted genes, its own seed) and
//! scored on every observation, so the days after the fit window are honest
//! forecasts and directly comparable with `assim` on the same days.
//!
//! `map` mode (validation E37) — MAP-Elites illumination of the spread genes
//! (`model.p0`, `model.burn_duration`, `wind_scale`): no objective, two
//! behaviour axes (growth of the burned area, elongation of its shape) over
//! `SMC_MAP_DAYS` days of the scenario's weather. The report is the archive:
//! which shapes and sizes the model can produce at all, next to the observed
//! perimeter's own growth and elongation on the same days.
//!
//! `replay` mode (validation E43 fix round 1) — a post-hoc diagnostic on a
//! `map`-mode archive (`SMC_MAP_REPLAY=<path>`, required): re-evaluates its
//! top elites (by elongation, among those at/above the observed size) and
//! reports connected-component stats ([`cella_lib::explore::metrics::largest_component_stats`])
//! on the burned set each one produces, so a "stretched" elongation number
//! can be told apart from a round core plus a few cells scattered far away
//! (spot-fire embers). See [`run_replay`]'s own doc comment.
//!
//! Usage (from cella_lib/):
//!   cargo run --release --example wildfire_smc -- <scenario_dir> <members> <open|assim|evolve|map|replay> <out.json>
//! Env: SMC_BETA (10), SMC_SIGMA (0.2), SMC_IMMIGRANTS (0), SMC_CROSSOVER (0), SMC_SEED (0),
//!      SMC_IMM_RESET=1 (immigrants start uncontained, with p0 from their own genome),
//!      SMC_IMM_RESET_GATE=<f64> (E39: gate the reset above on evidence — an
//!      immigrant is reset only while the last assimilation's area ratio
//!      (mean member burned area / observed burned area) is below this
//!      value, i.e. only while the population is under-predicting the
//!      observed area; setting this takes the decision away from
//!      SMC_IMM_RESET, which is then ignored),
//!      SMC_STATE_CORRECTION=immigrants|all (E40/E40b: a child's *grid*, not
//!      just its state, is rebuilt from the observed perimeter — burned
//!      cells become the model's burned type, the rim of still-unburned
//!      fuel next to a burned cell becomes burning at age 0, everything
//!      else is untouched from a fresh scenario grid; ignores
//!      SMC_IMM_RESET/SMC_IMM_RESET_GATE for the children it corrects,
//!      which are always uncontained. "immigrants" (E40) corrects only the
//!      immigrants; "all" (E40b) corrects every resampled child, which
//!      keeps its own learned/mutated genome, so learning continues while
//!      the grid is corrected every window. Default (and any other value):
//!      "none", the pre-E40 behaviour — a child's grid is a clone of a
//!      resampled parent, like any other child. SMC_IMM_SOURCE=observed is
//!      kept as an alias for SMC_STATE_CORRECTION=immigrants (E40's
//!      original runner uses it); SMC_STATE_CORRECTION wins if both are set,
//!      SMC_WIND_ROT_DEG (0), SMC_ASSIM_EVERY (1),
//!      SMC_PRIOR=path.json (a JSON array of genes replacing the default list),
//!      SMC_CONTAIN=1 (add the containment genes `contain_a`/`contain_b`, so
//!      the driver draws a containment once a day — the E28 operator),
//!      SMC_TAU_OFF=1 (drop the `tau_days` gene so containment is the only stop),
//!      SMC_FIT_DAYS (3), SMC_GENERATIONS (20 evolve / 30 map), SMC_POP (24 evolve /
//!      32 map batch), SMC_REPEATS (2), SMC_MAP_DAYS (5).
//!      SMC_MAX_DAYS=<n> (`open`/`assim` only, unset = run every observation
//!      day in the scenario: stop after the n-th scored day instead. Added
//!      for the 2026-09-12 ensemble-parallelism benchmark so a timing run
//!      doesn't pay for the whole ~23-30 day scenario every configuration;
//!      `SMC_MAP_DAYS` above is the pre-existing equivalent for `map` mode).
//!      SMC_SPOT=1 (E43: switch spotting on in the config's wildfire model
//!      — see [`enable_spotting`] — and add `model.spotting.p_spot` and
//!      `model.spotting.median_distance` to the gene list, so `map` mode
//!      illuminates the spread genes *and* spotting together),
//!      SMC_MAP_REPLAY=path.json (`replay` mode only, required: the
//!      `map`-mode report to re-evaluate), SMC_REPLAY_TOP (5 elites),
//!      SMC_REPLAY_SEEDS (3 seeds per elite).
//!      SMC_SPREAD=bernoulli|arrival, SMC_WIND_LAW=exponential|rear_focus,
//!      SMC_C2=<f64>, SMC_ARRIVAL_JITTER=<f64> (E30/E30a: override the
//!      scenario config's wildfire spread rule / wind law / c2 / jitter
//!      before the config is turned into a `Sim` — see [`configure_spread`].
//!      Each of the four is independent; any left unset keeps the scenario
//!      config's own value, so setting none of them is a no-op. E30's
//!      recommended setting is SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus,
//!      with `c2` and jitter left at the config's own defaults).
//!      SMC_STEPS_SCALE=<f64> (1; E30b: a faster clock — multiplies the
//!      scenario's own `steps_per_hour` before anything else reads it, so
//!      the forcing schedule's window step-counts and (since
//!      `steps_per_day` is computed from that same field) the wildfire
//!      driver's containment period both stretch together — one
//!      observation window still spans one day of weather at any scale.
//!      SMC_STEPS_SCALE=4 turns E30's 50 ticks/day into 200: the arrival
//!      rule's one-cell-per-tick cap rises from 1.5 km/day to 6 km/day).
//!      SMC_WIND_ROT_GENE=<f64> (off; E30b: half-width in degrees of a
//!      free, per-member `wind_rot_deg` gene, uniform on [-h, h] — added
//!      to the gene list only when this is set; see
//!      [`cella_lib::wildfire::driver::GENE_WIND_ROT_DEG`]. Each member
//!      learns its own offset to the forcing's wind *from*-bearing, on
//!      top of any fixed SMC_WIND_ROT_DEG rotation of the whole schedule
//!      above — the fixed knob rotates the input once for everyone, this
//!      one lets the filter search for a per-member correction to it).
//!
//! Default genes (the E25 prior): `model.p0` log-uniform 0.08–0.6,
//! `model.burn_duration` 5–20, `tau_days` log-uniform 2–100 days,
//! `wind_scale` 0–1.5.
//!
//! ## Module map
//!
//! `main` itself only parses the CLI/env and loads `scenario.json`/
//! `truth.json`/`config.json`, then dispatches to one of `modes::{open,
//! evolve, map}::run_*` (`assim` mode runs `modes::open::run` too — see
//! that module's own doc comment) or `nulls::run_nulls`. `knobs` centralises
//! the `SMC_*` env parsing; `priors` builds the gene list and the
//! config-time spread/spotting overrides; `nulls` holds every deterministic
//! dummy forecaster (persistence, Circle, Ellipse, their lagged variants);
//! `score` holds the per-observation report-row structs; `report` holds the
//! JSON-writing and build-provenance boilerplate every mode's report ends
//! with.

use std::path::{Path, PathBuf};

use cella_lib::config::CellaConfig;
use cella_lib::wildfire::driver::WeatherWindow;
use cella_lib::{CellType, GeneSpec};
use serde::Deserialize;

mod knobs;
mod modes;
mod nulls;
mod priors;
mod report;
mod score;

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

fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn mask_at(arrival: &[f64], t: f64) -> Vec<bool> {
    arrival.iter().map(|&a| a >= 0.0 && a <= t).collect()
}

/// The scenario's wind as a driver schedule (hours are absolute).
fn weather_schedule(sc: &Scenario, rot: f64) -> Vec<WeatherWindow> {
    sc.wind
        .iter()
        .map(|w| WeatherWindow {
            hours: w.hours,
            speed_ms: w.speed_ms,
            from_deg: w.from_deg + rot,
        })
        .collect()
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
    let knobs = knobs::Knobs::from_env();
    let prior: Vec<GeneSpec> = std::env::var("SMC_PRIOR")
        .ok()
        .map(|p| load(Path::new(&p)))
        .unwrap_or_else(priors::default_genes);
    let mut genes = priors::build_genes(
        prior,
        knobs.contain,
        knobs.tau_off,
        knobs.spot,
        knobs.wind_rot_gene,
    );

    let mut sc: Scenario = load(&dir.join("scenario.json"));
    knobs::apply_steps_scale(&mut sc, knobs.steps_scale);
    let truth: Truth = load(&dir.join("truth.json"));
    let mut cfg: CellaConfig = load(&dir.join("config.json"));
    priors::configure_spread(
        &mut cfg,
        knobs.spread.as_deref(),
        knobs.wind_law.as_deref(),
        knobs.c2,
        knobs.arrival_jitter,
    );
    if knobs.spot {
        priors::enable_spotting(&mut cfg);
    }
    assert_eq!(sc.format_version, 2, "unknown scenario format");
    assert_eq!(truth.format_version, 2, "unknown truth format");
    let burnt = [CellType::new("Burning"), CellType::new("BurnedOut")];
    let steps_per_day = knobs::steps_per_day_from(&sc);

    if mode == "map" {
        modes::map::run_map(
            &sc,
            &truth,
            &cfg,
            &out,
            &genes,
            knobs.wind_rot_deg,
            steps_per_day,
            knobs.seed,
            &burnt,
        );
        return;
    }
    if mode == "replay" {
        modes::map::run_replay(&sc, &cfg, &out, knobs.wind_rot_deg, steps_per_day, &burnt);
        return;
    }
    if mode == "nulls" {
        nulls::run_nulls(&sc, &truth, &dir, knobs.wind_rot_deg, &out);
        return;
    }

    let mut fit = None;
    if mode == "evolve" {
        let (report, best) = modes::evolve::fit_first_days(
            &sc,
            &truth,
            &cfg,
            &genes,
            knobs.wind_rot_deg,
            steps_per_day,
            knobs.seed,
            &burnt,
        );
        eprintln!(
            "fit over {} days: best mean IoU {:.3}, genome {:?}",
            report.fit_days, report.best_fit_iou, best
        );
        genes = genes
            .iter()
            .map(|g| knobs::pinned(&g.key, &best[&g.key]))
            .collect();
        fit = Some(report);
    }

    modes::open::run(
        &sc,
        &truth,
        &cfg,
        &out,
        &mode,
        members,
        genes,
        steps_per_day,
        &burnt,
        &knobs,
        fit,
    );
}
