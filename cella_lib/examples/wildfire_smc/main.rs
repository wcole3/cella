//! Ensemble forecasting for the wildfire model (validation E24 / E25 / E28),
//! built on the library's model-agnostic [`cella_lib::Ensemble`] plus the
//! [`cella_lib::wildfire::WildfireDriver`] — the same two pieces any other
//! model would use.
//!
//! Background for newcomers. An *ensemble* is a set of `M` simulation
//! "members" (also called *particles* in sequential Monte Carlo, SMC), each
//! with its own gene values (model settings such as `model.p0`, the base
//! ignition probability) and random seed. Running many members gives a
//! per-cell burn *probability* instead of one yes/no map. A forecast is
//! scored with IoU (intersection over union of the burned cell sets; 1 is a
//! perfect match) and the Brier score (mean squared error of the predicted
//! probability against the 0/1 outcome; lower is better). "Consensus" means
//! cells that at least half the members burned. *Resampling* means copying
//! well-scoring members over poorly-scoring ones; *immigrants* are fresh
//! random members injected to keep diversity. The scores are always printed
//! next to simple "null" forecasters (persistence, circle, ellipse; see
//! `nulls.rs`), because a model is only interesting where it beats them.
//!
//! # Modes
//!
//! `open` mode — plain Monte Carlo: `M` members with genes drawn from the
//! default gene list below, run independently; the per-cell burn probability
//! is scored as a probabilistic forecast (Brier, consensus IoU,
//! best-threshold IoU) beside the deterministic nulls (persistence,
//! area-matched radial).
//!
//! `assim` mode — "assimilation": after each observation the ensemble is
//! scored, then [`Ensemble::assimilate`] resamples, mutates and admits
//! immigrants, and the members keep simulating. Every score at t_k is a
//! forecast from the state assimilated at t_{k-1}: the mask at t_k is never
//! seen before it is scored.
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
//! `nulls` mode — scores only the deterministic null forecasters (no
//! ensemble); implemented in `nulls.rs` (`run_nulls`).
//!
//! # Usage (from cella_lib/)
//!
//! ```text
//! cargo run --release --example wildfire_smc -- <scenario_dir> <members> <mode> <out.json>
//! # e.g.
//! SMC_SEED=1 SMC_MAX_DAYS=5 cargo run --release --example wildfire_smc -- \
//!     ../validation/data/scenarios/Bear_2020 32 assim /tmp/bear_assim.json
//! ```
//!
//! `<mode>` is `open|assim|evolve|map|replay|nulls`. Only `<scenario_dir>`
//! (needs `scenario.json`, `truth.json`, `config.json`) is required. Defaults:
//! `<members>` 32 (ignored by `map`/`replay`/`nulls`), `<mode>` `open`,
//! `<out.json>` `smc_report.json`. Any other mode string is rejected: the
//! program prints the valid modes and exits with status 2.
//!
//! # Environment knobs
//!
//! All optional; a value that fails to parse is silently treated as unset
//! (except `SMC_CONTAIN_GROWTH_FLOOR`, which panics on an invalid number).
//! Boolean knobs (`SMC_IMM_RESET`, `SMC_SPOT`, `SMC_CONTAIN`, `SMC_TAU_OFF`,
//! `SMC_DIAG`) are on when set to a number above 0, e.g. `=1`.
//!
//! Ensemble / filter settings:
//! - `SMC_BETA` (10): selection sharpness for resampling; larger = the best
//!   members dominate more.
//! - `SMC_SIGMA` (0.2): size of the mutation applied to resampled children.
//! - `SMC_IMMIGRANTS` (0): fraction of fresh random members per assimilation.
//! - `SMC_CROSSOVER` (0): crossover (gene mixing between two parents) rate.
//! - `SMC_SEED` (0): ensemble / search seed.
//! - `SMC_ASSIM_EVERY` (1): assimilate every N observations (`assim` mode).
//! - `SMC_MAX_DAYS=<n>` (unset = every observation day in the scenario): stop
//!   after the n-th scored day. Applies to `open`, `assim` and `evolve`'s
//!   forecast half (not `map`; `SMC_MAP_DAYS` is its equivalent). Added for
//!   the 2026-09-12 ensemble-parallelism benchmark so a timing run does not
//!   pay for the whole ~23-30 day scenario.
//!
//! Immigrants and state correction (what a newly resampled member's grid and
//! driver state look like):
//! - `SMC_IMM_RESET=1`: immigrants start uncontained, with p0 from their own
//!   genome.
//! - `SMC_IMM_RESET_GATE=<f64>` (E39): gate that reset on evidence — an
//!   immigrant is reset only while the last assimilation's area ratio (mean
//!   member burned area / observed burned area) is below this value, i.e.
//!   only while the population is under-predicting the observed area. Setting
//!   it takes the decision away from `SMC_IMM_RESET`, which is then ignored.
//! - `SMC_STATE_CORRECTION=immigrants|all` (E40/E40b): a child's *grid*, not
//!   just its state, is rebuilt from the observed perimeter — burned cells
//!   become the model's burned type, the rim of still-unburned fuel next to a
//!   burned cell becomes burning at age 0, everything else is untouched from
//!   a fresh scenario grid. Such children are always uncontained, so
//!   `SMC_IMM_RESET`/`SMC_IMM_RESET_GATE` are ignored for them. `immigrants`
//!   (E40) corrects only the immigrants; `all` (E40b) corrects every resampled
//!   child, which keeps its own learned/mutated genome, so learning continues
//!   while the grid is corrected every window. Default (and any other value,
//!   including `none`): no correction — a child's grid is a clone of a
//!   resampled parent. `SMC_IMM_SOURCE=observed` is an older alias for
//!   `SMC_STATE_CORRECTION=immigrants` (E40's original runner uses it);
//!   `SMC_STATE_CORRECTION` wins if both are set.
//!
//! Genes:
//! - `SMC_PRIOR=path.json`: a JSON array of genes replacing the default list.
//! - `SMC_CONTAIN=1`: add the containment genes `contain_a`/`contain_b`, so
//!   the driver draws a containment once a day — the E28 operator.
//! - `SMC_TAU_OFF=1`: drop the `tau_days` gene so containment is the only stop.
//! - `SMC_SPOT=1` (E43): switch spotting (embers landing ahead of the front)
//!   on in the config's wildfire model — see [`enable_spotting`] — and add
//!   `model.spotting.p_spot` and `model.spotting.median_distance` to the gene
//!   list, so `map` mode illuminates the spread genes *and* spotting together.
//! - `SMC_WIND_ROT_GENE=<f64>` (off; E30b): half-width in degrees of a free,
//!   per-member `wind_rot_deg` gene, uniform on [-h, h], added to the gene
//!   list only when set; see
//!   [`cella_lib::wildfire::driver::GENE_WIND_ROT_DEG`]. Each member learns
//!   its own offset to the forcing's wind *from*-bearing, on top of any fixed
//!   `SMC_WIND_ROT_DEG` rotation (the fixed knob rotates the input once for
//!   everyone; this one lets the filter search for a per-member correction).
//! - `SMC_WIND_ROT_SIGMA=<f64>` (unset; Round 7 Task 5/E45): a per-gene
//!   mutation-size override on `wind_rot_deg` only; no effect unless
//!   `SMC_WIND_ROT_GENE` is also set — see [`priors::build_genes`]. Unset
//!   leaves the gene at the engine's own default sigma. `0` freezes the gene
//!   at each member's birth draw (resampling still copies it and selection
//!   still acts on it; only mutation stops), isolating per-member angular
//!   *diversity* from the *learning* half of the gene.
//!
//! Weather and clock:
//! - `SMC_WIND_ROT_DEG` (0): rotate the whole wind schedule by a fixed number
//!   of degrees.
//! - `SMC_STEPS_SCALE=<f64>` (1; E30b): a faster clock — multiplies the
//!   scenario's own `steps_per_hour` before anything else reads it, so the
//!   forcing schedule's window step-counts and the wildfire driver's
//!   containment period (`steps_per_day` is computed from the same field)
//!   stretch together; one observation window still spans one day of weather
//!   at any scale. `SMC_STEPS_SCALE=4` turns E30's 50 ticks/day into 200: the
//!   arrival rule's one-cell-per-tick cap rises from 1.5 km/day to 6 km/day.
//! - `SMC_WIND_SOURCE=era5|station` (era5; Round 7 Task 6/E46): which wind
//!   feeds the driver's per-window forcing in `open`/`assim`/`evolve`-forecast
//!   mode (`modes::open::run`). `station` replaces each window's ERA5 (speed,
//!   from-bearing) with the station log's vector mean over that same window —
//!   same daily cadence — falling back to that window's own ERA5 entry
//!   (counted as `station_fallback_windows` in the report) where the station
//!   log has no samples in range or no station file exists; see
//!   [`nulls::wind_schedule_for`]. `era5` leaves every pre-existing field's
//!   *value* unchanged, but the report is not byte-identical to a
//!   pre-Round-7 one, since it unconditionally gained `wind_source`,
//!   `station_fallback_windows` and `contain_growth_floor`. The deterministic
//!   nulls and `SMC_DIAG`'s `era5_*`/`station_*` fields always read the
//!   scenario's own ERA5 (or the station log directly) regardless of this
//!   knob; only the ensemble's own forcing changes.
//! - `SMC_STATION_WIND=<path>`: path of the station wind log used by
//!   `nulls` mode, `SMC_DIAG` and `SMC_WIND_SOURCE=station` (default: the
//!   scenario's own `station_hourly.json`; see `nulls::load_station`).
//!
//! Spread rule (E30/E30a): `SMC_SPREAD=bernoulli|arrival`,
//! `SMC_WIND_LAW=exponential|rear_focus`, `SMC_C2=<f64>`,
//! `SMC_ARRIVAL_JITTER=<f64>` override the scenario config's wildfire spread
//! rule / wind law / c2 / jitter before the config is turned into a `Sim` —
//! see [`configure_spread`]. Each of the four is independent; any left unset
//! keeps the scenario config's own value, so setting none of them is a no-op.
//! E30's recommended setting is `SMC_SPREAD=arrival SMC_WIND_LAW=rear_focus`
//! with `c2` and jitter left at the config's defaults.
//!
//! Containment: `SMC_CONTAIN_GROWTH_FLOOR=<f>` (1e-4; Round 7 Task 7/E49): the
//! floor the containment draw clamps a day's growth ratio to before taking its
//! log (`WildfireDriver::contain_growth_floor`). Unset is the value the
//! operator always used, so runs are unchanged; the floor in force is echoed
//! as the report's `contain_growth_floor` field. Must be finite and > 0.
//!
//! `evolve` mode: `SMC_FIT_DAYS` (3) observations to fit on, `SMC_GENERATIONS`
//! (20), `SMC_POP` (24) population, `SMC_REPEATS` (2) runs per genome.
//!
//! `map` mode: `SMC_MAP_DAYS` (5) days of weather, `SMC_GENERATIONS` (30),
//! `SMC_POP` (32, MAP-Elites batch size), `SMC_MAP_GROWTH_MAX` (0.1, upper end
//! of the growth behaviour axis).
//!
//! `replay` mode: `SMC_MAP_REPLAY=path.json` (required: the `map`-mode report
//! to re-evaluate), `SMC_REPLAY_TOP` (5 elites), `SMC_REPLAY_SEEDS` (3 seeds
//! per elite).
//!
//! Diagnostics: `SMC_DIAG=1` (off; E48/Round 7 Task 3): opt-in per-window
//! diagnostics on `ObsScore` — a new `diag` field, omitted from the JSON when
//! unset so every existing field is byte-identical to before this knob
//! existed. Emitted by `open`, `assim` and `evolve`'s forecast half alike,
//! since all three step through the same `modes::open::run` loop. See
//! [`diag`] for what it adds: the ERA5 wind vector and (when
//! `station_hourly.json` exists for the scenario) the station vector mean for
//! the window, the ensemble's per-window median `model.p0`/`wind_scale`/
//! `wind_rot_deg` (the last `None` unless `SMC_WIND_ROT_GENE` is set), the
//! per-window interquartile range (spread) of `wind_rot_deg`
//! (`wind_rot_deg_iqr`, `None` under the same rule), and a head-vs-flank
//! decomposition of the consensus-vs-truth miss and false-positive cells
//! (downwind of the ignition centroid, by the window's ERA5 "toward"
//! direction, vs cross/upwind). Round 7 Task 7 (E49) adds `contain_draws`
//! (every daily containment draw since the previous scored window: burned
//! count before/after, raw growth ratio before the floor, the member's
//! `contain_a`/`contain_b`, and whether it was contained) and
//! `min_growth_uncontained` (the smallest of those raw growths).
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
//! with; `diag` holds the opt-in (`SMC_DIAG=1`) per-window diagnostics
//! (E48) that `modes::open::run` attaches to `ObsScore.diag`.

use std::path::{Path, PathBuf};

use cella_lib::config::CellaConfig;
use cella_lib::wildfire::driver::WeatherWindow;
use cella_lib::{CellType, GeneSpec};
use serde::Deserialize;

mod diag;
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

#[derive(Deserialize, Clone, Copy)]
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

/// Every mode the example understands.
const MODES: [&str; 6] = ["open", "assim", "evolve", "map", "replay", "nulls"];

/// Checks the `<mode>` argument. `None` (not given) means `open`. A name that
/// is not in [`MODES`] is an error that lists the valid ones, so a typo does
/// not silently run the `open` loop.
fn parse_mode(arg: Option<&str>) -> Result<String, String> {
    let mode = arg.unwrap_or("open");
    if MODES.contains(&mode) {
        Ok(mode.to_string())
    } else {
        Err(format!(
            "unknown mode `{mode}`; valid modes are: {}",
            MODES.join(", ")
        ))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).expect("scenario dir"));
    let members: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(32);
    let mode = parse_mode(args.get(3).map(String::as_str)).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });
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
        knobs.wind_rot_sigma,
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
        knobs.warn_if_wind_or_floor_ignored("map mode");
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
        knobs.warn_if_wind_or_floor_ignored("evolve mode's fit half (it fits on ERA5)");
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
        &dir,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_mode_and_the_default_are_accepted() {
        for m in MODES {
            assert_eq!(parse_mode(Some(m)).unwrap(), m);
        }
        assert_eq!(parse_mode(None).unwrap(), "open");
    }

    #[test]
    fn unknown_mode_is_rejected_and_the_error_lists_the_valid_modes() {
        let err = parse_mode(Some("foo")).unwrap_err();
        assert!(err.contains("foo"), "{err}");
        for m in MODES {
            assert!(err.contains(m), "{err}");
        }
    }
}
