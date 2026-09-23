//! The Explore tab's engine room: state, the background worker, and the two
//! flows that cross between the worker and the main grid (assimilate the
//! grid you painted; apply a genome the search found).
//!
//! Nothing here names a model, a parameter key or a cell type. Genes come
//! from the grid's own parameter descriptions ([`cella_lib::Grid2D::params`]),
//! tracked types from its declared types, and the engines are the library's
//! model-agnostic [`Ensemble`] and [`Evolution`].
//!
//! The worker thread owns the engine. The UI thread never blocks on it: it
//! sends [`WorkerCmd`]s down one channel and drains [`WorkerMsg`]s from another
//! once per frame ([`CellaApp::poll_explore`]). Messages are throttled on the
//! worker side so a fast ensemble cannot flood the UI with probability maps.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cella_lib::explore::{
    ArchiveSnapshot, AssimilationReport, DescriptorSpec, Ensemble, EnsembleConfig, Evolution,
    EvolveConfig, GeneSpace, GeneSpec, GenerationReport, Genome, Goal, InitialCondition,
    MaskScore, Metric, Objective, Scale, Search, Selection, Sim, StateCorrection, When,
};
use cella_lib::rng::Rng;
use cella_lib::{CellType, GridState, ParamDesc, ParamKind, ParamValue};
use lasso2::Spur;

use super::app::{CellaApp, Dim};
use super::layers::ProbabilityMap;
use super::panels::explore::MAX_MEMBERS;
use super::state::{Notice, RULE_UNDO_CAP};

/// Which engine the tab is set up for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::gui) enum ExploreMode {
    #[default]
    MonteCarlo,
    Evolve,
}

/// One row of the genes table: a knob, whether it varies, and over what range.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct GeneRow {
    pub desc: ParamDesc,
    pub current: ParamValue,
    pub vary: bool,
    pub lo: f64,
    pub hi: f64,
    pub log: bool,
    /// For a `Choice` knob: which options are allowed (one flag per option).
    pub choices: Vec<bool>,
}

/// Build gene rows from a grid's knobs: every writable knob, not varying,
/// with its full declared range. Log scale is suggested for a positive
/// range spanning two decades or more.
pub(in crate::gui) fn gene_rows(
    descs: Vec<ParamDesc>,
    values: &HashMap<String, ParamValue>,
) -> Vec<GeneRow> {
    descs
        .into_iter()
        .filter(|d| !d.read_only)
        .filter_map(|desc| {
            let current = values.get(&desc.key)?.clone();
            let (lo, hi, log, choices) = match &desc.kind {
                ParamKind::Float { min, max, .. } => {
                    (*min, *max, *min > 0.0 && max / min >= 100.0, Vec::new())
                }
                ParamKind::Int { min, max } => (*min as f64, *max as f64, false, Vec::new()),
                ParamKind::Bool | ParamKind::Bits { .. } => (0.0, 1.0, false, Vec::new()),
                ParamKind::Choice { options } => (0.0, 0.0, false, vec![true; options.len()]),
            };
            Some(GeneRow {
                desc,
                current,
                vary: false,
                lo,
                hi,
                log,
                choices,
            })
        })
        .collect()
}

/// Keep the user's edits (vary, range, log, choices) for knobs that still
/// exist, add rows for new knobs, drop rows for knobs that are gone.
pub(in crate::gui) fn reconcile_genes(
    old: &[GeneRow],
    descs: Vec<ParamDesc>,
    values: &HashMap<String, ParamValue>,
) -> Vec<GeneRow> {
    gene_rows(descs, values)
        .into_iter()
        .map(|mut fresh| {
            if let Some(prev) = old
                .iter()
                .find(|r| r.desc.key == fresh.desc.key && r.desc.kind == fresh.desc.kind)
            {
                fresh.vary = prev.vary;
                fresh.lo = prev.lo;
                fresh.hi = prev.hi;
                fresh.log = prev.log;
                fresh.choices = prev.choices.clone();
            }
            fresh
        })
        .collect()
}

/// Order and clamp a typed range onto the knob's bounds.
pub(in crate::gui) fn clamp_range(lo: f64, hi: f64, kind: &ParamKind) -> (f64, f64) {
    let (min, max) = match kind {
        ParamKind::Float { min, max, .. } => (*min, *max),
        ParamKind::Int { min, max } => (*min as f64, *max as f64),
        _ => (0.0, 1.0),
    };
    let (a, b) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    (a.clamp(min, max), b.clamp(min, max))
}

/// The genes the user ticked, as the library's config form.
pub(in crate::gui) fn gene_specs(rows: &[GeneRow]) -> Vec<GeneSpec> {
    rows.iter()
        .filter(|r| r.vary)
        .map(|r| {
            let mut spec = GeneSpec::new(r.desc.key.clone());
            match &r.desc.kind {
                ParamKind::Float { .. } | ParamKind::Int { .. } => {
                    spec.range = Some([r.lo, r.hi]);
                    spec.scale = if r.log && r.lo > 0.0 {
                        Scale::Log
                    } else {
                        Scale::Linear
                    };
                }
                ParamKind::Choice { options } => {
                    let chosen: Vec<String> = options
                        .iter()
                        .zip(r.choices.iter().chain(std::iter::repeat(&true)))
                        .filter(|(_, on)| **on)
                        .map(|(o, _)| o.clone())
                        .collect();
                    if !chosen.is_empty() && chosen.len() < options.len() {
                        spec.choices = Some(chosen);
                    }
                }
                ParamKind::Bool | ParamKind::Bits { .. } => {}
            }
            spec
        })
        .collect()
}

/// Which measurement the Evolve objective and descriptors can pick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::gui) enum MetricChoice {
    #[default]
    Fraction,
    Activity,
    Entropy,
    Lifetime,
    /// Match the main grid's current mask of the tracked types.
    MatchGrid,
    BboxFraction,
    Elongation,
    CentroidSpeed,
    Growth,
    Period,
}

impl MetricChoice {
    pub(in crate::gui) const ALL: [MetricChoice; 10] = [
        MetricChoice::Fraction,
        MetricChoice::Activity,
        MetricChoice::Entropy,
        MetricChoice::Lifetime,
        MetricChoice::MatchGrid,
        MetricChoice::BboxFraction,
        MetricChoice::Elongation,
        MetricChoice::CentroidSpeed,
        MetricChoice::Growth,
        MetricChoice::Period,
    ];

    pub(in crate::gui) fn label(self) -> &'static str {
        match self {
            MetricChoice::Fraction => "Share of tracked types",
            MetricChoice::Activity => "Activity (cells changing)",
            MetricChoice::Entropy => "Entropy of the type mix",
            MetricChoice::Lifetime => "Lifetime (steps until still)",
            MetricChoice::MatchGrid => "Match the current grid",
            MetricChoice::BboxFraction => "Bounding-box share",
            MetricChoice::Elongation => "Elongation (shape)",
            MetricChoice::CentroidSpeed => "Centre-of-mass speed",
            MetricChoice::Growth => "Growth of tracked types",
            MetricChoice::Period => "Cycle length",
        }
    }

    /// Whether the metric needs the tracked types.
    pub(in crate::gui) fn needs_types(self) -> bool {
        matches!(
            self,
            MetricChoice::Fraction
                | MetricChoice::MatchGrid
                | MetricChoice::BboxFraction
                | MetricChoice::Elongation
                | MetricChoice::CentroidSpeed
                | MetricChoice::Growth
        )
    }

    /// Whether the metric can be an archive axis.
    pub(in crate::gui) fn is_descriptor(self) -> bool {
        !matches!(self, MetricChoice::MatchGrid)
    }

    /// The library metric, given the tracked type names and (for MatchGrid)
    /// the main grid's mask.
    pub(in crate::gui) fn to_metric(self, types: &[String], mask: &[bool]) -> Metric {
        let types = types.to_vec();
        match self {
            MetricChoice::Fraction => Metric::Fraction { types },
            MetricChoice::Activity => Metric::Activity,
            MetricChoice::Entropy => Metric::Entropy,
            MetricChoice::Lifetime => Metric::Lifetime,
            MetricChoice::MatchGrid => Metric::TargetMask {
                types,
                mask: mask.to_vec(),
                score: MaskScore::Iou,
            },
            MetricChoice::BboxFraction => Metric::BboxFraction { types },
            MetricChoice::Elongation => Metric::Elongation { types },
            MetricChoice::CentroidSpeed => Metric::CentroidSpeed { types },
            MetricChoice::Growth => Metric::Growth { types },
            MetricChoice::Period => Metric::Period { window: 64 },
        }
    }

    /// The picker entry for a library metric, or `None` when the panel has
    /// no entry for it. A saved block then keeps that metric exactly as it
    /// was loaded.
    pub(in crate::gui) fn from_metric(m: &Metric) -> Option<MetricChoice> {
        Some(match m {
            Metric::Fraction { .. } => MetricChoice::Fraction,
            Metric::Activity => MetricChoice::Activity,
            Metric::Entropy => MetricChoice::Entropy,
            Metric::Lifetime => MetricChoice::Lifetime,
            Metric::TargetMask { .. } => MetricChoice::MatchGrid,
            Metric::BboxFraction { .. } => MetricChoice::BboxFraction,
            Metric::Elongation { .. } => MetricChoice::Elongation,
            Metric::CentroidSpeed { .. } => MetricChoice::CentroidSpeed,
            Metric::Growth { .. } => MetricChoice::Growth,
            Metric::Period { window: 64 } => MetricChoice::Period,
            Metric::Period { .. } | Metric::Series { .. } | Metric::DensityClassification { .. } => {
                return None;
            }
        })
    }
}

/// The type names a metric measures, if it takes any.
fn metric_types(m: &Metric) -> Option<&[String]> {
    match m {
        Metric::Fraction { types }
        | Metric::TargetMask { types, .. }
        | Metric::Series { types, .. }
        | Metric::BboxFraction { types }
        | Metric::Elongation { types }
        | Metric::CentroidSpeed { types }
        | Metric::Growth { types } => Some(types),
        _ => None,
    }
}

/// A metric's name as the config file spells it (its `"metric"` tag).
fn metric_name(m: &Metric) -> String {
    serde_json::to_value(m)
        .ok()
        .and_then(|v| v.get("metric")?.as_str().map(str::to_string))
        .unwrap_or_else(|| "metric".into())
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::gui) enum GoalChoice {
    #[default]
    Maximise,
    Minimise,
    Target,
}

/// The objective picker's state.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct ObjectiveChoice {
    pub metric: MetricChoice,
    pub at_end: bool,
    pub at_step: u64,
    pub goal: GoalChoice,
    pub target: f64,
}

impl Default for ObjectiveChoice {
    fn default() -> Self {
        ObjectiveChoice {
            metric: MetricChoice::Fraction,
            at_end: true,
            at_step: 50,
            goal: GoalChoice::Maximise,
            target: 0.3,
        }
    }
}

impl ObjectiveChoice {
    pub(in crate::gui) fn to_objective(&self, types: &[String], mask: &[bool]) -> Objective {
        Objective {
            metric: self.metric.to_metric(types, mask),
            goal: match self.goal {
                GoalChoice::Maximise => Goal::Maximise,
                GoalChoice::Minimise => Goal::Minimise,
                GoalChoice::Target => Goal::Target(self.target),
            },
            when: if self.at_end {
                When::End
            } else {
                When::Step(self.at_step)
            },
        }
    }
}

/// Why the objective as configured cannot run, in words for the panel.
pub(in crate::gui) fn objective_error(
    obj: &ObjectiveChoice,
    steps: u64,
    tracked: bool,
) -> Option<String> {
    if obj.metric.needs_types() && !tracked {
        return Some("pick at least one tracked type for this metric".into());
    }
    if !obj.at_end && obj.at_step > steps {
        return Some(format!(
            "step {} is past the run length ({steps})",
            obj.at_step
        ));
    }
    if obj.goal == GoalChoice::Target && !obj.target.is_finite() {
        return Some("the target must be a number".into());
    }
    None
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::gui) enum SearchChoice {
    #[default]
    Objective,
    Novelty,
    MapElites,
}

/// One descriptor axis row in the Evolve controls.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct DescriptorRow {
    pub metric: MetricChoice,
    pub mean: bool,
    pub bins: u32,
}

impl Default for DescriptorRow {
    fn default() -> Self {
        DescriptorRow {
            metric: MetricChoice::Activity,
            mean: true,
            bins: 12,
        }
    }
}

/// Monte Carlo settings.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct McConfig {
    pub members: usize,
    pub seed: u64,
    pub steps: u64,
    pub beta: f64,
    pub sigma: f64,
    pub immigrants: f64,
}

impl Default for McConfig {
    fn default() -> Self {
        McConfig {
            members: 32,
            seed: 0,
            steps: 50,
            beta: 10.0,
            sigma: 0.2,
            immigrants: 0.2,
        }
    }
}

/// Evolve settings.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct EvoConfig {
    pub population: usize,
    pub generations: u32,
    pub seed: u64,
    pub steps: u64,
    pub repeats: usize,
    pub elite: usize,
    pub crossover: f64,
    pub mutation: f64,
    pub sigma: f64,
    pub immigrants: f64,
    pub search: SearchChoice,
    pub descriptors: Vec<DescriptorRow>,
}

impl Default for EvoConfig {
    fn default() -> Self {
        EvoConfig {
            population: 24,
            generations: 30,
            seed: 0,
            steps: 100,
            repeats: 2,
            elite: 2,
            crossover: 0.5,
            mutation: 0.3,
            sigma: 0.2,
            immigrants: 0.1,
            search: SearchChoice::Objective,
            descriptors: vec![
                DescriptorRow::default(),
                DescriptorRow {
                    metric: MetricChoice::Entropy,
                    mean: false,
                    bins: 12,
                },
            ],
        }
    }
}

/// What the UI asks the worker to do.
#[derive(Debug)]
pub(in crate::gui) enum WorkerCmd {
    RunSteps(u64),
    SetTracked(Vec<CellType>),
    Assimilate {
        observed: Vec<bool>,
        types: Vec<CellType>,
    },
    RunGenerations(u32),
    Stop,
}

/// What the worker reports back.
#[derive(Debug)]
pub(in crate::gui) enum WorkerMsg {
    Probability { steps: u64, cells: Vec<f32> },
    Assimilated(AssimilationReport),
    Generation(GenerationReport),
    Archive(ArchiveSnapshot),
    Done,
    Error(String),
}

/// A running worker thread and its channels.
pub(in crate::gui) struct ExploreWorker {
    pub tx: Sender<WorkerCmd>,
    pub rx: Receiver<WorkerMsg>,
    pub join: Option<JoinHandle<()>>,
    pub cancel: Arc<AtomicBool>,
    pub mode: ExploreMode,
    /// A command is in flight; buttons that would queue another are disabled.
    pub busy: bool,
    /// Dimension and size of the grid the worker was built from.
    pub sig: (Dim, usize, usize),
}

/// Everything the Explore tab remembers.
#[derive(Default)]
pub(in crate::gui) struct ExploreState {
    pub mode: ExploreMode,
    pub genes: Vec<GeneRow>,
    pub tracked: BTreeSet<Spur>,
    pub mc: McConfig,
    pub evo: EvoConfig,
    pub objective: ObjectiveChoice,
    pub worker: Option<ExploreWorker>,
    /// `(generation, best, mean)` per generation, for the chart.
    pub fitness: Vec<(u64, f64, f64)>,
    /// Best score and genome seen, by key.
    pub best: Option<(f64, Vec<(String, ParamValue)>)>,
    pub last_report: Option<AssimilationReport>,
    pub archive: Option<ArchiveSnapshot>,
    /// Thumbnail textures keyed by `(archive cell, snapshot generation)`.
    pub thumbs: HashMap<(usize, u64), egui::TextureHandle>,
    /// Step of the main grid when the ensemble was started.
    pub template_step: u64,
    /// Steps the ensemble has run.
    pub ensemble_steps: u64,
    pub gens_done: u32,
    pub gens_requested: u32,
    pub message: Option<String>,
    /// The egui context, remembered each frame so a worker can wake the UI.
    pub ctx: Option<egui::Context>,
    /// The `ensemble` block of the last loaded file (None for a demo or a
    /// file without one). Save and Run start from it, so fields this panel
    /// cannot edit survive.
    pub base_ensemble: Option<EnsembleConfig>,
    /// The same for the `evolve` block.
    pub base_evolve: Option<EvolveConfig>,
    /// What the panel could not show from the loaded blocks, for the
    /// notices. The saved block keeps those fields as loaded.
    pub kept_ensemble: Vec<String>,
    pub kept_evolve: Vec<String>,
    /// The panel right after the last scenario load. Save and Run compare
    /// against it to tell which settings the user changed since.
    pub panel_at_load: Option<PanelBaseline>,
    /// A run of that mode started since the last scenario load.
    pub ensemble_ran: bool,
    pub evolve_ran: bool,
}

/// A copy of the Explore panel taken right after a scenario load. Written by
/// [`CellaApp::explore_scenario_loaded`]; Save and Run diff the panel's
/// current settings against it (see [`CellaApp::current_ensemble_config`],
/// [`CellaApp::ensemble_block_for_save`]).
#[derive(Clone, Debug)]
pub(in crate::gui) struct PanelBaseline {
    /// What the ensemble builder produced, as JSON.
    pub ensemble: serde_json::Value,
    /// What the evolve builder produced, as JSON (None if it could not build).
    pub evolve: Option<serde_json::Value>,
    pub mc: McConfig,
    pub evo: EvoConfig,
    pub objective: ObjectiveChoice,
    /// JSON of the ticked genes (`gene_specs`).
    pub genes: serde_json::Value,
    pub tracked: BTreeSet<Spur>,
}

/// Rough memory an ensemble needs, in bytes: two cell buffers, ages and the
/// per-cell history per member.
pub(in crate::gui) fn ensemble_memory_estimate(
    members: usize,
    cells: usize,
    history_limit: usize,
) -> u64 {
    let per_cell = 4 + 4 + 4 + 4 * history_limit as u64;
    members as u64 * cells as u64 * per_cell
}

/// Largest ensemble this UI will start, in bytes (2 GiB).
pub(in crate::gui) const MAX_ENSEMBLE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Whether "Start" makes sense right now.
pub(in crate::gui) fn can_start(
    has_grid: bool,
    busy: bool,
    varying: usize,
    mode: ExploreMode,
    objective_ok: bool,
) -> bool {
    has_grid
        && !busy
        && match mode {
            ExploreMode::MonteCarlo => true,
            ExploreMode::Evolve => varying > 0 && objective_ok,
        }
}

/// Whether "Apply" makes sense right now.
pub(in crate::gui) fn can_apply(has_best: bool, playing: bool, busy: bool) -> bool {
    has_best && !playing && !busy
}

/// How far behind the main grid the ensemble is (0 when level or ahead).
pub(in crate::gui) fn steps_behind_main(
    template_step: u64,
    ensemble_steps: u64,
    current_step: u64,
) -> u64 {
    current_step.saturating_sub(template_step + ensemble_steps)
}

/// The main grid's cells that are in any tracked type, or `None` without a grid.
pub(in crate::gui) fn observation_mask(sim: &Sim, tracked: &BTreeSet<Spur>) -> Vec<bool> {
    sim.cells().iter().map(|c| tracked.contains(&c.0)).collect()
}

/// Apply one message to the state. Pure, so it is unit-testable; the layer
/// state gets the probability map.
pub(in crate::gui) fn handle_worker_msg(
    state: &mut ExploreState,
    layers: &mut super::layers::LayerState,
    msg: WorkerMsg,
) {
    let Some(worker) = state.worker.as_mut() else {
        return;
    };
    match msg {
        WorkerMsg::Probability { steps, cells } => {
            let (_, w, h) = worker.sig;
            if cells.len() == w * h {
                layers.probability_map = Some(ProbabilityMap {
                    width: w,
                    height: h,
                    cells,
                    steps,
                });
                layers.probability = true;
            }
            state.ensemble_steps = steps;
        }
        WorkerMsg::Assimilated(report) => {
            state.message = Some(format!(
                "learned: effective members {:.1}, immigrants {}, mean IoU {:.3}",
                report.effective_sample_size,
                report.immigrants,
                report.scores.iter().sum::<f64>() / report.scores.len().max(1) as f64
            ));
            state.last_report = Some(report);
        }
        WorkerMsg::Generation(r) => {
            state.fitness.push((r.generation, r.best, r.mean));
            state.gens_done += 1;
            if r.best.is_finite() && state.best.as_ref().is_none_or(|(b, _)| r.best >= *b) {
                state.best = Some((
                    r.best,
                    r.best_named
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ));
            }
            state.message = Some(match (&r.archive, r.novelty_mean) {
                (Some(a), _) => format!(
                    "generation {}: {} elites, coverage {:.0} %, QD {:.2}",
                    r.generation,
                    a.elites,
                    100.0 * a.coverage,
                    a.qd_score
                ),
                (None, Some(n)) => format!(
                    "generation {}: novelty {:.3}, archive {}",
                    r.generation,
                    n,
                    r.archive_size.unwrap_or(0)
                ),
                _ => format!(
                    "generation {}: best {:.4} mean {:.4}",
                    r.generation, r.best, r.mean
                ),
            });
        }
        WorkerMsg::Archive(snap) => {
            state.thumbs.clear();
            state.archive = Some(snap);
        }
        WorkerMsg::Done => worker.busy = false,
        WorkerMsg::Error(e) => {
            worker.busy = false;
            state.message = Some(format!("Explore error: {e}"));
        }
    }
}

/// How long the worker waits between probability maps.
const PROBABILITY_EVERY: Duration = Duration::from_millis(50);
/// How long the worker waits between archive snapshots.
const ARCHIVE_EVERY: Duration = Duration::from_millis(500);

/// Start an ensemble on a background thread. The ensemble is built inside
/// the thread so a large clone never blocks the UI.
pub(in crate::gui) fn spawn_monte_carlo(
    template: Sim,
    cfg: EnsembleConfig,
    mut tracked: Vec<CellType>,
    ctx: Option<egui::Context>,
    sig: (Dim, usize, usize),
) -> ExploreWorker {
    let (cmd_tx, cmd_rx) = channel::<WorkerCmd>();
    let (msg_tx, msg_rx) = channel::<WorkerMsg>();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_in = cancel.clone();
    let join = std::thread::spawn(move || {
        let wake = |ctx: &Option<egui::Context>| {
            if let Some(c) = ctx {
                c.request_repaint();
            }
        };
        let mut ens = match Ensemble::new(template, &cfg) {
            Ok(e) => e,
            Err(e) => {
                let _ = msg_tx.send(WorkerMsg::Error(e.to_string()));
                wake(&ctx);
                return;
            }
        };
        let send_prob = |ens: &Ensemble, tracked: &[CellType]| {
            let _ = msg_tx.send(WorkerMsg::Probability {
                steps: ens.step_count(),
                cells: ens.state_probability(tracked),
            });
        };
        send_prob(&ens, &tracked);
        let _ = msg_tx.send(WorkerMsg::Done);
        wake(&ctx);
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                WorkerCmd::RunSteps(n) => {
                    let mut done = 0u64;
                    let mut last = Instant::now();
                    while done < n && !cancel_in.load(Ordering::Relaxed) {
                        let chunk = (n - done).min(10);
                        if let Err(e) = ens.step_n(chunk) {
                            let _ = msg_tx.send(WorkerMsg::Error(e.to_string()));
                            break;
                        }
                        done += chunk;
                        if last.elapsed() >= PROBABILITY_EVERY {
                            send_prob(&ens, &tracked);
                            wake(&ctx);
                            last = Instant::now();
                        }
                    }
                    cancel_in.store(false, Ordering::Relaxed);
                    send_prob(&ens, &tracked);
                }
                WorkerCmd::SetTracked(t) => {
                    tracked = t;
                    send_prob(&ens, &tracked);
                }
                WorkerCmd::Assimilate { observed, types } => {
                    match ens.assimilate(&observed, &types) {
                        Ok(report) => {
                            let _ = msg_tx.send(WorkerMsg::Assimilated(report));
                            send_prob(&ens, &tracked);
                        }
                        Err(e) => {
                            let _ = msg_tx.send(WorkerMsg::Error(e.to_string()));
                        }
                    }
                }
                WorkerCmd::RunGenerations(_) => {}
                WorkerCmd::Stop => break,
            }
            let _ = msg_tx.send(WorkerMsg::Done);
            wake(&ctx);
        }
    });
    ExploreWorker {
        tx: cmd_tx,
        rx: msg_rx,
        join: Some(join),
        cancel,
        mode: ExploreMode::MonteCarlo,
        busy: true,
        sig,
    }
}

/// Start an evolution on a background thread.
pub(in crate::gui) fn spawn_evolve(
    template: Sim,
    cfg: EvolveConfig,
    ctx: Option<egui::Context>,
    sig: (Dim, usize, usize),
) -> ExploreWorker {
    let (cmd_tx, cmd_rx) = channel::<WorkerCmd>();
    let (msg_tx, msg_rx) = channel::<WorkerMsg>();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_in = cancel.clone();
    let join = std::thread::spawn(move || {
        let wake = |ctx: &Option<egui::Context>| {
            if let Some(c) = ctx {
                c.request_repaint();
            }
        };
        let mut evo = match Evolution::new(template, &cfg) {
            Ok(e) => e,
            Err(e) => {
                let _ = msg_tx.send(WorkerMsg::Error(e.to_string()));
                wake(&ctx);
                return;
            }
        };
        let _ = msg_tx.send(WorkerMsg::Done);
        wake(&ctx);
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                WorkerCmd::RunGenerations(g) => {
                    let mut last_archive = Instant::now() - ARCHIVE_EVERY;
                    for _ in 0..g {
                        if cancel_in.load(Ordering::Relaxed) {
                            break;
                        }
                        let report = evo.step_generation();
                        let _ = msg_tx.send(WorkerMsg::Generation(report));
                        if last_archive.elapsed() >= ARCHIVE_EVERY
                            && let Some(snap) = evo.archive_snapshot()
                        {
                            let _ = msg_tx.send(WorkerMsg::Archive(snap));
                            last_archive = Instant::now();
                        }
                        wake(&ctx);
                    }
                    cancel_in.store(false, Ordering::Relaxed);
                    if let Some(snap) = evo.archive_snapshot() {
                        let _ = msg_tx.send(WorkerMsg::Archive(snap));
                    }
                }
                WorkerCmd::Stop => break,
                WorkerCmd::RunSteps(_)
                | WorkerCmd::SetTracked(_)
                | WorkerCmd::Assimilate { .. } => {}
            }
            let _ = msg_tx.send(WorkerMsg::Done);
            wake(&ctx);
        }
    });
    ExploreWorker {
        tx: cmd_tx,
        rx: msg_rx,
        join: Some(join),
        cancel,
        mode: ExploreMode::Evolve,
        busy: true,
        sig,
    }
}

impl CellaApp {
    /// A clone of the main grid as the engines' template, with its dimension
    /// signature.
    pub(in crate::gui) fn template_sim(&self) -> Option<(Sim, (Dim, usize, usize))> {
        match self.scenario.dim? {
            Dim::D1 => self
                .scenario
                .d1
                .as_ref()
                .map(|g| (Sim::D1(g.clone()), (Dim::D1, g.width, 1))),
            Dim::D2 => self
                .scenario
                .d2
                .as_ref()
                .map(|g| (Sim::D2(g.clone()), (Dim::D2, g.width, g.height))),
        }
    }

    /// The main grid's knobs and their current values.
    pub(in crate::gui) fn grid_knobs(&self) -> (Vec<ParamDesc>, HashMap<String, ParamValue>) {
        let descs = match self.scenario.dim {
            Some(Dim::D1) => self
                .scenario
                .d1
                .as_ref()
                .map(|g| g.params())
                .unwrap_or_default(),
            Some(Dim::D2) => self
                .scenario
                .d2
                .as_ref()
                .map(|g| g.params())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let values = descs
            .iter()
            .filter_map(|d| {
                let v = match self.scenario.dim {
                    Some(Dim::D1) => self.scenario.d1.as_ref()?.get_param(&d.key),
                    Some(Dim::D2) => self.scenario.d2.as_ref()?.get_param(&d.key),
                    None => None,
                }?;
                Some((d.key.clone(), v))
            })
            .collect();
        (descs, values)
    }

    /// Bring the gene rows and tracked set in line with the grid as it is now.
    pub(in crate::gui) fn reconcile_explore_state(&mut self) {
        let (descs, values) = self.grid_knobs();
        let rows = reconcile_genes(&self.explore.genes, descs, &values);
        self.explore.genes = rows;
        let declared: BTreeSet<Spur> = self
            .declared_types()
            .into_iter()
            .filter(|t| *t != CellType::inactive())
            .map(|t| t.0)
            .collect();
        self.explore.tracked.retain(|t| declared.contains(t));
        if self.explore.tracked.is_empty() {
            self.explore.tracked = declared;
        }
    }

    /// Tracked types as a list.
    pub(in crate::gui) fn tracked_types(&self) -> Vec<CellType> {
        self.explore.tracked.iter().map(|s| CellType(*s)).collect()
    }

    /// Drain the worker's messages, and drop a worker whose thread has gone.
    pub(in crate::gui) fn poll_explore(&mut self) {
        let Some(worker) = self.explore.worker.as_ref() else {
            return;
        };
        // A worker built from a different grid is stale.
        let stale = self.template_sim().is_none_or(|(_, sig)| sig != worker.sig);
        let mut msgs = Vec::new();
        let mut disconnected = false;
        for _ in 0..16 {
            match worker.rx.try_recv() {
                Ok(m) => msgs.push(m),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        for m in msgs {
            handle_worker_msg(&mut self.explore, &mut self.view.layers, m);
        }
        let dead = self
            .explore
            .worker
            .as_ref()
            .is_some_and(|w| w.join.as_ref().is_some_and(|j| j.is_finished()) && w.busy);
        if disconnected || stale || dead {
            self.explore_on_grid_replaced();
            if !stale {
                self.set_status("Explore worker stopped unexpectedly");
            }
        }
    }

    /// The grid was replaced (load, resize, reset): stop and drop the worker
    /// and the map it produced. Fitness history and the best genome stay so
    /// Reset → Apply → Play still works.
    pub(in crate::gui) fn explore_on_grid_replaced(&mut self) {
        if let Some(w) = self.explore.worker.take() {
            w.cancel.store(true, Ordering::Relaxed);
            let _ = w.tx.send(WorkerCmd::Stop);
            // Never join on the UI thread; the thread ends on its own.
        }
        self.view.layers.probability_map = None;
        self.explore.last_report = None;
        self.explore.template_step = 0;
        self.explore.ensemble_steps = 0;
    }

    /// Write a genome into the main grid, all or nothing: every knob is set on
    /// a clone first, and the clone replaces the grid only if all succeed.
    /// The Reset snapshot receives the same values so Reset keeps the genome.
    pub(in crate::gui) fn apply_genome(
        &mut self,
        pairs: &[(String, ParamValue)],
    ) -> Result<(), String> {
        let Some((mut sim, _)) = self.template_sim() else {
            return Err("no grid loaded".into());
        };
        for (k, v) in pairs {
            sim.set_param(k, v.clone())
                .map_err(|e| format!("'{k}': {e}"))?;
        }
        self.push_rule_undo(pairs.iter().map(|(k, _)| k.as_str()));
        match sim {
            Sim::D1(g) => self.scenario.d1 = Some(g),
            Sim::D2(g) => self.scenario.d2 = Some(g),
        }
        self.mirror_params_into_snapshot(pairs);
        self.refresh_rule_editor_from_current();
        Ok(())
    }

    /// Remember the current value of each key so Undo rule can restore it.
    /// Keys the grid does not know are skipped.
    fn push_rule_undo<'a>(&mut self, keys: impl Iterator<Item = &'a str>) {
        let Some((sim, _)) = self.template_sim() else {
            return;
        };
        let before: Vec<(String, ParamValue)> = keys
            .filter_map(|k| sim.get_param(k).map(|v| (k.to_string(), v)))
            .collect();
        if before.is_empty() {
            return;
        }
        self.edit.rule_undo.push(before);
        if self.edit.rule_undo.len() > RULE_UNDO_CAP {
            self.edit.rule_undo.remove(0);
        }
    }

    /// Every knob the grid lets us write, as genes with their declared bounds.
    fn all_knob_space(&self, sim: &Sim) -> Result<GeneSpace, String> {
        let specs: Vec<GeneSpec> = sim
            .params()
            .into_iter()
            .filter(|d| !d.read_only)
            .map(|d| GeneSpec::new(d.key))
            .collect();
        if specs.is_empty() {
            return Err("this simulation has no adjustable knobs".into());
        }
        GeneSpace::resolve(&specs, sim, &[], &[]).map_err(|e| e.to_string())
    }

    /// Write a whole genome through the gene space (which repairs `limit`
    /// against `count`), all or nothing, and record the undo.
    fn apply_space_genome(
        &mut self,
        space: &GeneSpace,
        genome: &Genome,
    ) -> Result<Vec<(String, ParamValue)>, String> {
        let Some((mut sim, _)) = self.template_sim() else {
            return Err("no grid loaded".into());
        };
        space.apply(&mut sim, genome).map_err(|e| e.to_string())?;
        // Read back what actually landed (after repair) for the snapshot.
        let landed: Vec<(String, ParamValue)> = space
            .genes()
            .iter()
            .filter_map(|g| sim.get_param(&g.key).map(|v| (g.key.clone(), v)))
            .collect();
        self.push_rule_undo(landed.iter().map(|(k, _)| k.as_str()));
        match sim {
            Sim::D1(g) => self.scenario.d1 = Some(g),
            Sim::D2(g) => self.scenario.d2 = Some(g),
        }
        self.mirror_params_into_snapshot(&landed);
        self.refresh_rule_editor_from_current();
        Ok(landed)
    }

    /// Surprise me: random knobs, then a random fill with the same seed.
    pub(in crate::gui) fn surprise_me(&mut self, seed: u64) {
        if self.playback.playing {
            self.set_status("Pause before a surprise");
            return;
        }
        let Some((sim, _)) = self.template_sim() else {
            self.set_status("Load a simulation first");
            return;
        };
        let space = match self.all_knob_space(&sim) {
            Ok(s) => s,
            Err(e) => {
                self.set_status(format!("Surprise me: {e}"));
                return;
            }
        };
        let genome = space.sample(&mut Rng::new(seed));
        match self.apply_space_genome(&space, &genome) {
            Ok(landed) => {
                let fill_type = self
                    .declared_types()
                    .into_iter()
                    .find(|t| *t != CellType::inactive());
                if let Some(ty) = fill_type {
                    self.push(super::actions::Action::RandomFill {
                        density: 0.3,
                        ty,
                        seed,
                        clear_first: true,
                    });
                }
                self.set_status(format!("Surprise (seed {seed}): {}", pairs_text(&landed)));
            }
            Err(e) => self.set_status(format!("Surprise me failed: {e}")),
        }
    }

    /// Mutate rule: nudge every knob from where it is now.
    pub(in crate::gui) fn mutate_rule(&mut self, seed: u64, sigma: f64) {
        if self.playback.playing {
            self.set_status("Pause before mutating the rule");
            return;
        }
        let Some((sim, _)) = self.template_sim() else {
            self.set_status("Load a simulation first");
            return;
        };
        let space = match self.all_knob_space(&sim) {
            Ok(s) => s,
            Err(e) => {
                self.set_status(format!("Mutate rule: {e}"));
                return;
            }
        };
        let mut genome = space.from_sim(&sim);
        let before = genome.clone();
        space.mutate(&mut Rng::new(seed), &mut genome, sigma.clamp(0.0, 1.0));
        if genome == before {
            self.set_status("Mutation left every knob unchanged; try a larger sigma");
            return;
        }
        match self.apply_space_genome(&space, &genome) {
            Ok(landed) => {
                let changed: Vec<(String, ParamValue)> = landed
                    .into_iter()
                    .filter(|(k, v)| sim.get_param(k).as_ref() != Some(v))
                    .collect();
                self.set_status(format!("Mutated (seed {seed}): {}", pairs_text(&changed)));
            }
            Err(e) => self.set_status(format!("Mutate rule failed: {e}")),
        }
    }

    /// Undo rule: restore the knob values recorded before the last change.
    pub(in crate::gui) fn undo_rule(&mut self) {
        if self.playback.playing {
            self.set_status("Pause before undoing a rule change");
            return;
        }
        let Some(pairs) = self.edit.rule_undo.pop() else {
            self.set_status("Nothing to undo in the rule");
            return;
        };
        let Some((mut sim, _)) = self.template_sim() else {
            return;
        };
        for (k, v) in &pairs {
            if let Err(e) = sim.set_param(k, v.clone()) {
                self.set_status(format!("Undo rule failed at '{k}': {e}"));
                return;
            }
        }
        match sim {
            Sim::D1(g) => self.scenario.d1 = Some(g),
            Sim::D2(g) => self.scenario.d2 = Some(g),
        }
        self.mirror_params_into_snapshot(&pairs);
        self.refresh_rule_editor_from_current();
        self.set_status(format!("Rule restored: {}", pairs_text(&pairs)));
    }

    /// The Edit tab's fill type, or the first non-background type.
    pub(in crate::gui) fn fill_type_or_default(&self) -> Option<CellType> {
        let types: Vec<CellType> = self
            .declared_types()
            .into_iter()
            .filter(|t| *t != CellType::inactive())
            .collect();
        match self.edit.fill_type {
            Some(t) if types.contains(&t) => Some(t),
            _ => types.first().copied(),
        }
    }

    /// The Edit tab's seed, then bump it so the next keypress differs.
    pub(in crate::gui) fn next_fill_seed(&mut self) -> u64 {
        let seed = self.edit.fill_seed;
        self.edit.fill_seed = seed.wrapping_add(1);
        seed
    }

    /// Copy accepted knob values into the Reset snapshot, whichever side of
    /// the grid they belong to (rule or model).
    pub(in crate::gui) fn mirror_params_into_snapshot(&mut self, pairs: &[(String, ParamValue)]) {
        let Some(state) = self.scenario.initial_state.as_ref() else {
            return;
        };
        let Some(mut sim) = Sim::from_state(state) else {
            return;
        };
        for (k, v) in pairs {
            let _ = sim.set_param(k, v.clone());
        }
        self.scenario.initial_state = Some(match &sim {
            Sim::D1(g) => GridState::from_grid1d(g),
            Sim::D2(g) => GridState::from_grid2d(g),
        });
    }

    /// Tracked type names, for building metrics.
    fn tracked_names(&self) -> Vec<String> {
        self.tracked_types()
            .iter()
            .map(|t| t.as_str().to_string())
            .collect()
    }

    /// The Evolve objective as configured, against the current grid.
    pub(in crate::gui) fn current_objective(&self) -> Option<Objective> {
        let (sim, _) = self.template_sim()?;
        let mask = observation_mask(&sim, &self.explore.tracked);
        Some(
            self.explore
                .objective
                .to_objective(&self.tracked_names(), &mask),
        )
    }

    /// The evolve block the Evolve controls describe.
    pub(in crate::gui) fn panel_evolve_config(&self) -> Option<EvolveConfig> {
        let evo = &self.explore.evo;
        let objective = self.current_objective()?;
        let names = self.tracked_names();
        let descriptors: Vec<DescriptorSpec> = evo
            .descriptors
            .iter()
            .filter(|d| d.metric.is_descriptor())
            .map(|d| DescriptorSpec {
                metric: d.metric.to_metric(&names, &[]),
                when: if d.mean { When::Mean } else { When::End },
                range: None,
                bins: d.bins.max(1),
            })
            .collect();
        let search = match evo.search {
            SearchChoice::Objective => Search::Objective,
            SearchChoice::Novelty => Search::Novelty {
                k: 15,
                threshold: None,
            },
            SearchChoice::MapElites => Search::MapElites {
                batch: evo.population.max(2),
                iso_line: true,
            },
        };
        Some(EvolveConfig {
            population: evo.population.max(2),
            generations: evo.generations as usize,
            seed: evo.seed,
            genes: gene_specs(&self.explore.genes),
            objective: Some(objective),
            search,
            descriptors: if evo.search == SearchChoice::Objective {
                Vec::new()
            } else {
                descriptors
            },
            thumbnails: true,
            steps: evo.steps.max(1),
            repeats: evo.repeats.max(1),
            elite: evo.elite.min(evo.population.saturating_sub(1)),
            crossover: evo.crossover,
            mutation: evo.mutation,
            sigma: evo.sigma,
            immigrants: evo.immigrants,
            ..EvolveConfig::default()
        })
    }

    /// The ensemble block the Monte Carlo controls describe.
    pub(in crate::gui) fn panel_ensemble_config(&self) -> EnsembleConfig {
        let mc = &self.explore.mc;
        EnsembleConfig {
            members: mc.members.max(1),
            seed: mc.seed,
            genes: gene_specs(&self.explore.genes),
            track: self.tracked_names(),
            beta: mc.beta,
            sigma: mc.sigma,
            immigrants: mc.immigrants,
            crossover: 0.0,
            immigrant_reset: false,
            immigrant_reset_gate: None,
            state_correction: StateCorrection::None,
            driver: None,
        }
    }

    /// The ensemble block Run and Save use. With **no** loaded block, this is
    /// simply [`Self::panel_ensemble_config`] as it is right now — there is
    /// nothing to merge over, so nothing from load time is frozen into it (a
    /// "match the current grid" mask, for instance, always reflects the live
    /// grid). With a loaded block, it is the panel's edits since the last
    /// load, merged over that block (see [`overlay_json`]).
    pub(in crate::gui) fn current_ensemble_config(&self) -> EnsembleConfig {
        let now = self.panel_ensemble_config();
        let Some(base) = self.explore.base_ensemble.as_ref() else {
            return now;
        };
        let Some(at_load) = self.explore.panel_at_load.as_ref().map(|b| &b.ensemble) else {
            return now;
        };
        // Deserializing can only fail on a value the panel itself produced
        // wrongly; fall back to the panel's own config rather than lose Run.
        let (Ok(now_v), Ok(base_v)) = (serde_json::to_value(&now), serde_json::to_value(base))
        else {
            return now;
        };
        serde_json::from_value(overlay_json(&base_v, at_load, &now_v)).unwrap_or(now)
    }

    /// The evolve block Run and Save use; see [`Self::current_ensemble_config`].
    pub(in crate::gui) fn current_evolve_config(&self) -> Option<EvolveConfig> {
        let now = self.panel_evolve_config()?;
        let Some(base) = self.explore.base_evolve.as_ref() else {
            return Some(now);
        };
        let Some(at_load) = self.explore.panel_at_load.as_ref().and_then(|b| b.evolve.as_ref())
        else {
            return Some(now);
        };
        let (Ok(now_v), Ok(base_v)) = (serde_json::to_value(&now), serde_json::to_value(base))
        else {
            return Some(now);
        };
        Some(serde_json::from_value(overlay_json(&base_v, at_load, &now_v)).unwrap_or(now))
    }

    /// Whether the gene table or tracked set changed since the last load.
    /// Both modes share them, so an edit counts for the mode selected now.
    fn shared_edited(&self) -> bool {
        let Some(b) = &self.explore.panel_at_load else {
            return false;
        };
        serde_json::to_value(gene_specs(&self.explore.genes)).unwrap_or_default() != b.genes
            || self.explore.tracked != b.tracked
    }

    /// The ensemble block to save, or `None` for a session that never used
    /// it: written when the file had one, a run started, or the user changed
    /// an ensemble setting since the load. ("Run +N" steps don't count.)
    pub(in crate::gui) fn ensemble_block_for_save(&self) -> Option<EnsembleConfig> {
        let x = &self.explore;
        let edited = x.panel_at_load.as_ref().is_some_and(|b| {
            McConfig { steps: 0, ..x.mc.clone() } != McConfig { steps: 0, ..b.mc.clone() }
                || (x.mode == ExploreMode::MonteCarlo && self.shared_edited())
        });
        (x.base_ensemble.is_some() || x.ensemble_ran || edited).then(|| self.current_ensemble_config())
    }

    /// The evolve block to save; see [`Self::ensemble_block_for_save`].
    pub(in crate::gui) fn evolve_block_for_save(&self) -> Option<EvolveConfig> {
        let x = &self.explore;
        let edited = x.panel_at_load.as_ref().is_some_and(|b| {
            x.evo != b.evo
                || x.objective != b.objective
                || (x.mode == ExploreMode::Evolve && self.shared_edited())
        });
        if x.base_evolve.is_some() || x.evolve_ran || edited {
            self.current_evolve_config()
        } else {
            None
        }
    }

    /// The Explore side of a scenario load: show the file's blocks in the
    /// panel, remember them as the base for Save and Run, take a fresh
    /// baseline, and pop up a notice when a block was loaded. Every scenario
    /// load calls this, including demos with `(None, None)`, so a previous
    /// file's base never leaks into the next scenario.
    pub(in crate::gui) fn explore_scenario_loaded(
        &mut self,
        ensemble: Option<EnsembleConfig>,
        evolve: Option<EvolveConfig>,
    ) {
        self.reconcile_explore_state();
        self.explore.kept_ensemble.clear();
        self.explore.kept_evolve.clear();
        let mut lines = Vec::new();
        // Evolve first, then ensemble: the gene table and tracked set are
        // shared, and when a file has both, the ensemble's win (its mode is
        // selected). The evolve block keeps its own genes through the merge.
        if let Some(e) = &evolve {
            self.explore.kept_evolve = self.apply_evolve_to_panel(e);
            self.explore.mode = ExploreMode::Evolve;
            lines.push(format!(
                "Evolve: population {}, {} generations, {} genes.",
                e.population, e.generations, e.genes.len()
            ));
            if !self.explore.kept_evolve.is_empty() {
                lines.push(format!(
                    "Kept as-is (not editable here): {}.",
                    self.explore.kept_evolve.join("; ")
                ));
            }
        }
        if let Some(e) = &ensemble {
            self.explore.kept_ensemble = self.apply_ensemble_to_panel(e);
            self.explore.mode = ExploreMode::MonteCarlo;
            let track = if e.track.is_empty() { "all types".to_string() } else { e.track.join(", ") };
            lines.insert(0, format!(
                "Ensemble: {} members, {} genes, tracking {track}.",
                e.members, e.genes.len()
            ));
            if !self.explore.kept_ensemble.is_empty() {
                lines.insert(1, format!(
                    "Kept as-is (not editable here): {}.",
                    self.explore.kept_ensemble.join("; ")
                ));
            }
        }
        self.explore.base_ensemble = ensemble;
        self.explore.base_evolve = evolve;
        self.explore.ensemble_ran = false;
        self.explore.evolve_ran = false;
        self.explore.panel_at_load = Some(self.panel_baseline());
        if !lines.is_empty() {
            self.chrome.notice = Some(Notice {
                title: "Explore settings loaded".into(),
                lines,
            });
        }
    }

    /// Snapshot the panel as it is now (see [`PanelBaseline`]).
    fn panel_baseline(&self) -> PanelBaseline {
        PanelBaseline {
            ensemble: serde_json::to_value(self.panel_ensemble_config()).unwrap_or_default(),
            evolve: self.panel_evolve_config().and_then(|c| serde_json::to_value(c).ok()),
            mc: self.explore.mc.clone(),
            evo: self.explore.evo.clone(),
            objective: self.explore.objective.clone(),
            genes: serde_json::to_value(gene_specs(&self.explore.genes)).unwrap_or_default(),
            tracked: self.explore.tracked.clone(),
        }
    }

    /// Tick the gene rows a block names and set their ranges; untick every
    /// other row. Returns a note naming each gene that has no row here (a
    /// free, driver or wildcard gene); the saved block keeps those as loaded.
    fn apply_genes_to_panel(&mut self, genes: &[GeneSpec]) -> Vec<String> {
        for row in &mut self.explore.genes {
            row.vary = false;
        }
        let mut no_row = Vec::new();
        for spec in genes {
            let Some(row) = self.explore.genes.iter_mut().find(|r| r.desc.key == spec.key) else {
                no_row.push(spec.key.clone());
                continue;
            };
            row.vary = true;
            match &row.desc.kind {
                ParamKind::Float { .. } | ParamKind::Int { .. } => {
                    // `range: None` means "use the knob's full declared
                    // range" (see `GeneSpec::new`'s docs), not "leave
                    // whatever this row had from the previous scenario".
                    (row.lo, row.hi) = match spec.range {
                        Some([lo, hi]) => clamp_range(lo, hi, &row.desc.kind),
                        None => match &row.desc.kind {
                            ParamKind::Float { min, max, .. } => (*min, *max),
                            ParamKind::Int { min, max } => (*min as f64, *max as f64),
                            _ => (row.lo, row.hi),
                        },
                    };
                    row.log = matches!(spec.scale, Scale::Log);
                }
                ParamKind::Choice { options } => {
                    row.choices = match &spec.choices {
                        Some(chosen) => options.iter().map(|o| chosen.contains(o)).collect(),
                        None => vec![true; options.len()],
                    };
                }
                ParamKind::Bool | ParamKind::Bits { .. } => {}
            }
        }
        if no_row.is_empty() {
            Vec::new()
        } else {
            vec![format!("{} gene(s) with no row here: {}", no_row.len(), no_row.join(", "))]
        }
    }

    /// Track the named types; an empty list means every declared type.
    /// Names the grid does not declare are ignored.
    fn apply_track_to_panel(&mut self, names: &[String]) {
        let declared: BTreeSet<Spur> = self
            .declared_types()
            .into_iter()
            .filter(|t| *t != CellType::inactive())
            .map(|t| t.0)
            .collect();
        let chosen: BTreeSet<Spur> = names
            .iter()
            .map(|n| CellType::new(n).0)
            .filter(|s| declared.contains(s))
            .collect();
        self.explore.tracked = if chosen.is_empty() { declared } else { chosen };
    }

    /// Show a loaded ensemble block in the Monte Carlo controls. Returns what
    /// the panel cannot show.
    fn apply_ensemble_to_panel(&mut self, e: &EnsembleConfig) -> Vec<String> {
        let mut kept = self.apply_genes_to_panel(&e.genes);
        self.apply_track_to_panel(&e.track);
        let mc = &mut self.explore.mc;
        mc.members = e.members.clamp(1, MAX_MEMBERS);
        mc.seed = e.seed;
        mc.beta = e.beta;
        mc.sigma = e.sigma;
        mc.immigrants = e.immigrants;
        if e.members > MAX_MEMBERS {
            kept.push(format!("{} members (the panel shows at most {MAX_MEMBERS})", e.members));
        }
        if e.driver.is_some() {
            kept.push("driver".into());
        }
        if e.crossover != 0.0 {
            kept.push(format!("crossover {}", e.crossover));
        }
        if e.immigrant_reset {
            kept.push("immigrant reset".into());
        }
        if e.immigrant_reset_gate.is_some() {
            kept.push("immigrant reset gate".into());
        }
        if !matches!(e.state_correction, StateCorrection::None) {
            kept.push("state correction".into());
        }
        kept
    }

    /// Show a loaded evolve block in the Evolve controls. Returns what the
    /// panel cannot show.
    fn apply_evolve_to_panel(&mut self, e: &EvolveConfig) -> Vec<String> {
        let mut kept = self.apply_genes_to_panel(&e.genes);
        {
            let evo = &mut self.explore.evo;
            evo.population = e.population;
            evo.generations = e.generations.min(u32::MAX as usize) as u32;
            evo.seed = e.seed;
            evo.steps = e.steps;
            evo.repeats = e.repeats;
            evo.elite = e.elite;
            evo.crossover = e.crossover;
            evo.mutation = e.mutation;
            evo.sigma = e.sigma;
            evo.immigrants = e.immigrants;
            evo.search = match &e.search {
                Search::Objective => SearchChoice::Objective,
                Search::Novelty { k, threshold } => {
                    if *k != 15 || threshold.is_some() {
                        kept.push(format!("novelty settings (k {k})"));
                    }
                    SearchChoice::Novelty
                }
                Search::MapElites { batch, iso_line } => {
                    kept.push(format!("map-elites batch {batch}, iso-line {iso_line}"));
                    SearchChoice::MapElites
                }
            };
            let mut rows = Vec::new();
            for d in &e.descriptors {
                match (MetricChoice::from_metric(&d.metric), &d.when) {
                    (Some(metric), When::Mean | When::End) if d.range.is_none() => {
                        rows.push(DescriptorRow { metric, mean: matches!(d.when, When::Mean), bins: d.bins });
                    }
                    _ => kept.push(format!("descriptor '{}'", metric_name(&d.metric))),
                }
            }
            if !rows.is_empty() {
                evo.descriptors = rows;
            }
        }
        match &e.objective {
            Some(o) => match MetricChoice::from_metric(&o.metric) {
                Some(metric) => {
                    let obj = &mut self.explore.objective;
                    obj.metric = metric;
                    obj.goal = match o.goal {
                        Goal::Maximise => GoalChoice::Maximise,
                        Goal::Minimise => GoalChoice::Minimise,
                        Goal::Target(t) => {
                            obj.target = t;
                            GoalChoice::Target
                        }
                    };
                    match o.when {
                        When::End => obj.at_end = true,
                        When::Step(k) => {
                            obj.at_end = false;
                            obj.at_step = k;
                        }
                        When::Mean => kept.push("objective averaged over the run".into()),
                    }
                    if matches!(o.metric, Metric::TargetMask { .. }) {
                        kept.push("objective target mask".into());
                    }
                    if let Some(types) = metric_types(&o.metric) {
                        let types = types.to_vec();
                        self.apply_track_to_panel(&types);
                    }
                }
                None => kept.push(format!("objective '{}'", metric_name(&o.metric))),
            },
            None => kept.push("no objective (archive only)".into()),
        }
        if !matches!(e.selection, Selection::Tournament { k: 3 }) {
            kept.push("selection".into());
        }
        if !matches!(e.initial, InitialCondition::Fixed) {
            kept.push("random initial grids".into());
        }
        if e.driver.is_some() {
            kept.push("driver".into());
        }
        if !e.forcing.is_empty() {
            kept.push("forcing".into());
        }
        if !e.thumbnails {
            kept.push("thumbnails off".into());
        }
        kept
    }
}

/// `key = value, key = value` for status lines.
pub(in crate::gui) fn pairs_text(pairs: &[(String, ParamValue)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k} = {}", super::panels::model::value_text(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The Explore tab's requests that touch the worker or the main grid.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) enum ExploreAction {
    SetMode(ExploreMode),
    SetTracked(CellType, bool),
    SetVary(usize, bool),
    VaryAll(bool),
    /// Build an ensemble from the main grid and start its worker.
    StartMonteCarlo,
    /// Advance the ensemble by `n` steps.
    RunSteps(u64),
    /// Advance the ensemble until it is level with the main grid.
    RunToMain,
    /// Score every member against the main grid's tracked cells and resample.
    Assimilate,
    /// Build an evolution from the main grid and start its worker.
    StartEvolve,
    RunGenerations(u32),
    /// Interrupt the current command; the worker stays.
    Stop,
    /// Drop the worker and everything it produced.
    Discard,
    /// Write the best genome found into the main grid.
    ApplyBest,
    /// Write one archive cell's genome into the main grid.
    ApplyElite(usize),
}

impl CellaApp {
    /// The reducer arm for [`Action::Explore`](super::actions::Action::Explore).
    pub(in crate::gui) fn apply_explore_action(&mut self, action: ExploreAction) {
        match action {
            ExploreAction::SetMode(m) => self.explore.mode = m,
            ExploreAction::SetTracked(t, on) => {
                if on {
                    self.explore.tracked.insert(t.0);
                } else {
                    self.explore.tracked.remove(&t.0);
                }
                let types = self.tracked_types();
                if let Some(w) = self.explore.worker.as_ref()
                    && w.mode == ExploreMode::MonteCarlo
                {
                    let _ = w.tx.send(WorkerCmd::SetTracked(types));
                }
            }
            ExploreAction::SetVary(i, on) => {
                if let Some(r) = self.explore.genes.get_mut(i) {
                    r.vary = on;
                }
            }
            ExploreAction::VaryAll(on) => self.explore.genes.iter_mut().for_each(|r| r.vary = on),
            ExploreAction::StartMonteCarlo => self.start_monte_carlo(),
            ExploreAction::RunSteps(n) => self.send_worker(WorkerCmd::RunSteps(n)),
            ExploreAction::RunToMain => {
                let behind = steps_behind_main(
                    self.explore.template_step,
                    self.explore.ensemble_steps,
                    self.current_step(),
                );
                if behind > 0 {
                    self.send_worker(WorkerCmd::RunSteps(behind));
                } else {
                    self.set_status("Ensemble is already level with the grid");
                }
            }
            ExploreAction::Assimilate => {
                let Some((sim, _)) = self.template_sim() else {
                    return;
                };
                let observed = observation_mask(&sim, &self.explore.tracked);
                let types = self.tracked_types();
                let behind = steps_behind_main(
                    self.explore.template_step,
                    self.explore.ensemble_steps,
                    self.current_step(),
                );
                if behind > 0 {
                    self.set_status(format!(
                        "Assimilating while the ensemble is {behind} steps behind the grid"
                    ));
                }
                self.send_worker(WorkerCmd::Assimilate { observed, types });
            }
            ExploreAction::StartEvolve => self.start_evolve(),
            ExploreAction::RunGenerations(g) => {
                self.explore.gens_requested = self.explore.gens_done + g;
                self.send_worker(WorkerCmd::RunGenerations(g));
            }
            ExploreAction::Stop => {
                if let Some(w) = &self.explore.worker {
                    w.cancel.store(true, Ordering::Relaxed);
                    self.set_status("Stopping after the current batch");
                }
            }
            ExploreAction::Discard => {
                self.explore_on_grid_replaced();
                self.explore.fitness.clear();
                self.explore.best = None;
                self.explore.archive = None;
                self.explore.thumbs.clear();
                self.explore.gens_done = 0;
                self.explore.gens_requested = 0;
                self.explore.message = None;
                self.set_status("Explore results discarded");
            }
            ExploreAction::ApplyBest => {
                let Some((_, pairs)) = self.explore.best.clone() else {
                    return;
                };
                self.apply_pairs_with_status(&pairs, "best genome");
            }
            ExploreAction::ApplyElite(i) => {
                let pairs: Option<Vec<(String, ParamValue)>> = self
                    .explore
                    .archive
                    .as_ref()
                    .and_then(|a| a.cells.get(i).cloned().flatten())
                    .map(|c| c.named.into_iter().collect());
                let Some(pairs) = pairs else {
                    self.set_status("That archive cell is empty");
                    return;
                };
                self.apply_pairs_with_status(&pairs, &format!("elite {i}"));
            }
        }
    }

    fn apply_pairs_with_status(&mut self, pairs: &[(String, ParamValue)], what: &str) {
        if self.playback.playing {
            self.set_status("Pause before applying a genome");
            return;
        }
        match self.apply_genome(pairs) {
            Ok(()) => self.set_status(format!("Applied {what}: {}", pairs_text(pairs))),
            Err(e) => self.set_status(format!("Could not apply {what}: {e}")),
        }
    }

    fn send_worker(&mut self, cmd: WorkerCmd) {
        let Some(w) = self.explore.worker.as_mut() else {
            self.set_status("Start an ensemble or evolution first");
            return;
        };
        if w.busy {
            self.set_status("The worker is still busy");
            return;
        }
        w.busy = true;
        if w.tx.send(cmd).is_err() {
            self.explore_on_grid_replaced();
            self.set_status("Explore worker is gone; start again");
        }
    }

    /// Members × cells, refused above [`MAX_ENSEMBLE_BYTES`].
    fn start_monte_carlo(&mut self) {
        let Some((template, sig)) = self.template_sim() else {
            self.set_status("Load a simulation first");
            return;
        };
        let history = match &template {
            Sim::D1(g) => g.history_limit,
            Sim::D2(g) => g.history_limit,
        };
        let bytes = ensemble_memory_estimate(self.explore.mc.members, template.len(), history);
        if bytes > MAX_ENSEMBLE_BYTES {
            self.set_status(format!(
                "Ensemble too large (~{} MiB); fewer members or a smaller grid",
                bytes >> 20
            ));
            return;
        }
        self.explore_on_grid_replaced();
        let cfg = self.current_ensemble_config();
        let tracked = self.tracked_types();
        let ctx = self.explore.ctx.clone();
        self.explore.template_step = self.current_step();
        self.explore.ensemble_steps = 0;
        self.explore.worker = Some(spawn_monte_carlo(template, cfg, tracked, ctx, sig));
        self.explore.ensemble_ran = true;
        self.view.layers.probability = true;
        self.set_status(format!(
            "Started {} members; probability layer on",
            self.explore.mc.members
        ));
    }

    fn start_evolve(&mut self) {
        let Some((template, sig)) = self.template_sim() else {
            self.set_status("Load a simulation first");
            return;
        };
        if let Some(e) = objective_error(
            &self.explore.objective,
            self.explore.evo.steps,
            !self.explore.tracked.is_empty(),
        ) {
            self.set_status(format!("Objective: {e}"));
            return;
        }
        let Some(cfg) = self.current_evolve_config() else {
            return;
        };
        self.explore_on_grid_replaced();
        self.explore.fitness.clear();
        self.explore.best = None;
        self.explore.archive = None;
        self.explore.thumbs.clear();
        self.explore.gens_done = 0;
        self.explore.gens_requested = 0;
        let ctx = self.explore.ctx.clone();
        self.explore.worker = Some(spawn_evolve(template, cfg, ctx, sig));
        self.explore.evolve_ran = true;
        self.set_status(format!(
            "Started evolution: population {}, {} varying genes",
            self.explore.evo.population,
            self.explore.genes.iter().filter(|r| r.vary).count()
        ));
    }
}

/// Merge the panel's config over a loaded base, one top-level field at a
/// time. A field takes the panel's value only if the user changed it since
/// the load (`now` differs from `at_load`); every other field keeps the
/// base's value. That protects what the panel can show only approximately:
/// a loaded driver, an objective metric with no picker entry, a gene's
/// `sigma`. `genes` merge by key (see [`merge_genes`]). Mask arrays are
/// ignored when comparing (see [`without_masks`]).
pub(in crate::gui) fn overlay_json(
    base: &serde_json::Value,
    at_load: &serde_json::Value,
    now: &serde_json::Value,
) -> serde_json::Value {
    let mut out = base.clone();
    let (Some(map), Some(now_map)) = (out.as_object_mut(), now.as_object()) else {
        return now.clone();
    };
    for (k, v) in now_map {
        if k == "genes" {
            continue;
        }
        let changed = at_load.get(k).map(without_masks) != Some(without_masks(v));
        if changed {
            map.insert(k.clone(), v.clone());
        }
    }
    map.insert(
        "genes".into(),
        merge_genes(base.get("genes"), at_load.get("genes"), now.get("genes")),
    );
    out
}

/// Merge gene lists by `key`. B = base genes, L = panel genes at load,
/// N = panel genes now:
/// - a base gene whose key is in N takes N's version if it changed since
///   load, otherwise stays as loaded;
/// - a base gene missing from N but present in L was unticked, so it is
///   dropped;
/// - a base gene in neither (a gene with no panel row) is kept;
/// - a gene in N that is not in B, and is new or changed since load, is
///   appended.
fn merge_genes(
    base: Option<&serde_json::Value>,
    at_load: Option<&serde_json::Value>,
    now: Option<&serde_json::Value>,
) -> serde_json::Value {
    use serde_json::Value;
    let list = |v: Option<&Value>| v.and_then(Value::as_array).cloned().unwrap_or_default();
    let key = |g: &Value| g.get("key").and_then(Value::as_str).map(str::to_string);
    let find = |gs: &[Value], k: &str| gs.iter().find(|g| key(g).as_deref() == Some(k)).cloned();
    let (b, l, n) = (list(base), list(at_load), list(now));
    let mut out = Vec::new();
    for g in &b {
        let Some(k) = key(g) else {
            out.push(g.clone());
            continue;
        };
        match (find(&n, &k), find(&l, &k)) {
            (Some(now_g), Some(load_g)) if now_g != load_g => out.push(now_g),
            (Some(_), Some(_)) => out.push(g.clone()),
            (Some(now_g), None) => out.push(now_g),
            (None, Some(_)) => {}
            (None, None) => out.push(g.clone()),
        }
    }
    for g in &n {
        if let Some(k) = key(g)
            && find(&b, &k).is_none()
            && find(&l, &k).as_ref() != Some(g)
        {
            out.push(g.clone());
        }
    }
    Value::Array(out)
}

/// A copy of `v` with every `"mask"` field removed, at any depth. The
/// "match the current grid" objective bakes the live grid's mask into the
/// config, and that changes on every step; comparing without it stops a
/// step from counting as an edit.
fn without_masks(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| k.as_str() != "mask")
                .map(|(k, x)| (k.clone(), without_masks(x)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_masks).collect()),
        other => other.clone(),
    }
}

/// Crop or pad top-left with `false`, in place, every `"mask"` array in `v`
/// (at any depth) whose length equals `old`'s cell count (`old.0 * old.1`),
/// to `new`'s — the same rule a grid itself resizes by
/// ([`cella_lib::resize::remap_blocks`]).
///
/// A loaded evolve block's "match the current grid" objective carries one
/// flag per cell of the grid it was saved against (a user's target, not
/// something read off the live grid — see [`CellaApp::current_evolve_config`]
/// for the case with no loaded block, which always uses the live grid's
/// mask instead). After the grid is resized in the GUI, that stale-length
/// mask must move with it: left alone, a run refuses it ("target mask: wrong
/// length") and a save writes a file whose mask disagrees with its own width
/// and height. Recurses like [`without_masks`], but rewrites a mask instead
/// of stripping it.
pub(in crate::gui) fn resize_masks(
    v: &mut serde_json::Value,
    old: (usize, usize),
    new: (usize, usize),
) {
    use serde_json::Value;
    match v {
        Value::Object(m) => {
            let remapped = match m.get("mask") {
                Some(Value::Array(arr)) if arr.len() == old.0 * old.1 => arr
                    .iter()
                    .map(Value::as_bool)
                    .collect::<Option<Vec<bool>>>()
                    .map(|bools| cella_lib::resize::remap_blocks(&bools, 1, old, new, false)),
                _ => None,
            };
            if let Some(bools) = remapped {
                m.insert(
                    "mask".to_string(),
                    Value::Array(bools.into_iter().map(Value::Bool).collect()),
                );
            }
            for (k, x) in m.iter_mut() {
                if k != "mask" {
                    resize_masks(x, old, new);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| resize_masks(x, old, new)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::sim::tests::test_app;
    use serde_json::json;

    fn life_app() -> CellaApp {
        let mut app = test_app();
        app.load_demo_life();
        app.reconcile_explore_state();
        app
    }

    #[test]
    fn overlay_keeps_unchanged_fields_from_the_base_and_takes_edited_ones() {
        let base = json!({"members": 64, "beta": 5.0, "driver": {"d": 1}, "genes": []});
        let at_load = json!({"members": 64, "beta": 5.0, "genes": []});
        let now = json!({"members": 64, "beta": 7.0, "genes": []});
        let out = overlay_json(&base, &at_load, &now);
        assert_eq!(out["members"], 64);
        assert_eq!(out["beta"], 7.0);
        assert_eq!(out["driver"], json!({"d": 1}), "a field the panel lacks survives");
    }

    #[test]
    fn overlay_ignores_a_baked_in_mask_that_only_moved_with_the_grid() {
        let base = json!({"objective": {"metric": "density_classification"}, "genes": []});
        let at_load = json!({"objective": {"metric": "target_mask", "mask": [true, false]}, "genes": []});
        let now = json!({"objective": {"metric": "target_mask", "mask": [false, false]}, "genes": []});
        let out = overlay_json(&base, &at_load, &now);
        assert_eq!(out["objective"]["metric"], "density_classification");
    }

    #[test]
    fn genes_merge_by_key() {
        let g = |k: &str, lo: f64| json!({"key": k, "range": [lo, 1.0]});
        let base = json!({"genes": [g("p0", 0.1), g("free", 0.0), g("gone", 0.2), g("kept", 0.3)]});
        let at_load = json!({"genes": [g("p0", 0.1), g("gone", 0.2), g("kept", 0.3)]});
        let now = json!({"genes": [g("p0", 0.5), g("kept", 0.3), g("new", 0.4)]});
        let out = overlay_json(&base, &at_load, &now);
        assert_eq!(
            out["genes"],
            json!([g("p0", 0.5), g("free", 0.0), g("kept", 0.3), g("new", 0.4)])
        );
    }

    #[test]
    fn an_unchanged_base_gene_keeps_fields_the_panel_cannot_show() {
        let base = json!({"genes": [{"key": "p0", "range": [0.1, 1.0], "sigma": 0.01}]});
        let at_load = json!({"genes": [{"key": "p0", "range": [0.1, 1.0]}]});
        let now = at_load.clone();
        let out = overlay_json(&base, &at_load, &now);
        assert_eq!(out["genes"][0]["sigma"], 0.01);
    }

    #[test]
    fn every_metric_choice_round_trips_through_the_library_metric() {
        for c in MetricChoice::ALL {
            let m = c.to_metric(&["A".to_string()], &[true]);
            assert_eq!(MetricChoice::from_metric(&m), Some(c), "{c:?}");
        }
        assert_eq!(MetricChoice::from_metric(&Metric::Period { window: 32 }), None);
    }

    #[test]
    fn apply_genes_to_panel_resets_a_none_range_to_the_knobs_declared_bounds() {
        let mut app = life_app();
        // Pretend a previous scenario narrowed this row's range.
        let row = app
            .explore
            .genes
            .iter_mut()
            .find(|r| r.desc.key == "rule.subrules[0].count")
            .unwrap();
        row.lo = 3.0;
        row.hi = 5.0;
        // A `GeneSpec::new` gene has `range: None`: "use the knob's full
        // declared range" per its docs, not "leave the row's stale range".
        let spec = GeneSpec::new("rule.subrules[0].count".to_string());
        assert!(spec.range.is_none());
        app.apply_genes_to_panel(&[spec]);
        let row = app
            .explore
            .genes
            .iter()
            .find(|r| r.desc.key == "rule.subrules[0].count")
            .unwrap();
        assert_eq!(
            (row.lo, row.hi),
            (0.0, 8.0),
            "a None range should reset to the knob's declared bounds, not keep the stale narrow one"
        );
    }

    #[test]
    fn gene_rows_come_from_the_grid_and_keep_edits_across_reconcile() {
        let app = life_app();
        let rows = &app.explore.genes;
        assert!(rows.iter().any(|r| r.desc.key == "rule.subrules[0].count"));
        assert!(rows.iter().all(|r| !r.vary && !r.desc.read_only));
        let count = rows
            .iter()
            .find(|r| r.desc.key == "rule.subrules[0].count")
            .unwrap();
        assert_eq!((count.lo, count.hi), (0.0, 8.0));
        let op = rows
            .iter()
            .find(|r| r.desc.key == "rule.subrules[0].op")
            .unwrap();
        assert_eq!(op.choices, vec![true; 3]);
        // Edits survive a reconcile; a vanished knob is dropped; a new one appears.
        let mut edited = rows.clone();
        edited[0].vary = true;
        edited[0].lo = 3.0;
        edited[0].hi = 5.0;
        let (descs, values) = app.grid_knobs();
        let again = reconcile_genes(&edited, descs, &values);
        assert!(again[0].vary && again[0].lo == 3.0 && again[0].hi == 5.0);
        let (mut descs, values) = app.grid_knobs();
        descs.retain(|d| d.key != "rule.subrules[0].count");
        let fewer = reconcile_genes(&edited, descs, &values);
        assert!(fewer.iter().all(|r| r.desc.key != "rule.subrules[0].count"));
        assert_eq!(
            clamp_range(9.0, -2.0, &ParamKind::Int { min: 0, max: 8 }),
            (0.0, 8.0)
        );
        assert_eq!(
            clamp_range(
                0.2,
                0.7,
                &ParamKind::Float {
                    min: 0.0,
                    max: 1.0,
                    step: 0.1
                }
            ),
            (0.2, 0.7)
        );
        assert_eq!(clamp_range(0.9, 0.1, &ParamKind::Bool), (0.1, 0.9));
        // Specs: only ticked rows, with their ranges; a full choice set is left implicit.
        let mut rows = edited;
        rows.iter_mut()
            .find(|r| r.desc.key == "rule.subrules[0].op")
            .unwrap()
            .vary = true;
        let specs = gene_specs(&rows);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].range, Some([3.0, 5.0]));
        assert!(
            specs
                .iter()
                .find(|s| s.key == "rule.subrules[0].op")
                .unwrap()
                .choices
                .is_none()
        );
        rows.iter_mut()
            .find(|r| r.desc.key == "rule.subrules[0].op")
            .unwrap()
            .choices = vec![true, false, true];
        let specs = gene_specs(&rows);
        assert_eq!(
            specs
                .iter()
                .find(|s| s.key == "rule.subrules[0].op")
                .unwrap()
                .choices
                .as_ref()
                .unwrap()
                .len(),
            2
        );
        // Tracked defaults to every non-background type.
        assert_eq!(app.tracked_types(), vec![CellType::from("Alive")]);
        let mask = observation_mask(&app.template_sim().unwrap().0, &app.explore.tracked);
        assert_eq!(
            mask.iter().filter(|b| **b).count(),
            3,
            "the Life demo starts with a blinker"
        );
    }

    #[test]
    fn objective_and_config_builders_follow_the_pickers() {
        let mut app = life_app();
        let obj = app.current_objective().unwrap();
        assert_eq!(
            obj.metric,
            Metric::Fraction {
                types: vec!["Alive".into()]
            }
        );
        assert_eq!(obj.when, When::End);
        app.explore.objective = ObjectiveChoice {
            metric: MetricChoice::MatchGrid,
            at_end: false,
            at_step: 7,
            goal: GoalChoice::Target,
            target: 0.5,
        };
        let obj = app.current_objective().unwrap();
        assert!(matches!(obj.metric, Metric::TargetMask { ref mask, .. } if mask.len() == 1500));
        assert_eq!(obj.when, When::Step(7));
        assert_eq!(obj.goal, Goal::Target(0.5));
        assert_eq!(objective_error(&app.explore.objective, 100, true), None);
        assert!(
            objective_error(&app.explore.objective, 5, true)
                .unwrap()
                .contains("past the run length")
        );
        assert!(
            objective_error(&app.explore.objective, 100, false)
                .unwrap()
                .contains("tracked")
        );
        app.explore.objective.target = f64::NAN;
        assert!(
            objective_error(&app.explore.objective, 100, true)
                .unwrap()
                .contains("number")
        );
        for m in MetricChoice::ALL {
            assert!(!m.label().is_empty());
            let _ = m.to_metric(&["A".into()], &[true]);
        }
        assert!(!MetricChoice::MatchGrid.is_descriptor() && MetricChoice::Period.is_descriptor());
        // Ensemble and evolve configs pick up ticked genes and the search mode.
        app.explore.genes[0].vary = true;
        let ens = app.current_ensemble_config();
        assert_eq!(ens.genes.len(), 1);
        assert_eq!(ens.track, vec!["Alive".to_string()]);
        app.explore.evo.search = SearchChoice::MapElites;
        let evo = app.current_evolve_config().unwrap();
        assert!(matches!(evo.search, Search::MapElites { .. }));
        assert_eq!(evo.descriptors.len(), 2);
        app.explore.evo.search = SearchChoice::Objective;
        assert!(app.current_evolve_config().unwrap().descriptors.is_empty());
        assert!(can_start(true, false, 1, ExploreMode::Evolve, true));
        assert!(!can_start(true, false, 0, ExploreMode::Evolve, true));
        assert!(can_start(true, false, 0, ExploreMode::MonteCarlo, false));
        assert!(!can_start(true, true, 1, ExploreMode::MonteCarlo, true));
        assert!(
            can_apply(true, false, false)
                && !can_apply(true, true, false)
                && !can_apply(false, false, false)
        );
        assert_eq!(steps_behind_main(10, 5, 20), 5);
        assert_eq!(steps_behind_main(10, 15, 20), 0);
        assert_eq!(
            ensemble_memory_estimate(32, 1_000_000, 0),
            32 * 1_000_000 * 12
        );
    }

    #[test]
    fn worker_messages_update_state_and_the_probability_layer() {
        let mut app = life_app();
        let (tx, _rx_cmd) = channel::<WorkerCmd>();
        let (_tx_msg, rx) = channel::<WorkerMsg>();
        app.explore.worker = Some(ExploreWorker {
            tx,
            rx,
            join: None,
            cancel: Arc::new(AtomicBool::new(false)),
            mode: ExploreMode::MonteCarlo,
            busy: true,
            sig: (Dim::D2, 50, 30),
        });
        let cells = vec![0.5f32; 1500];
        handle_worker_msg(
            &mut app.explore,
            &mut app.view.layers,
            WorkerMsg::Probability { steps: 4, cells },
        );
        assert_eq!(app.view.layers.probability_map.as_ref().unwrap().steps, 4);
        assert_eq!(app.explore.ensemble_steps, 4);
        handle_worker_msg(
            &mut app.explore,
            &mut app.view.layers,
            WorkerMsg::Probability {
                steps: 5,
                cells: vec![0.5; 3],
            },
        );
        assert_eq!(
            app.view.layers.probability_map.as_ref().unwrap().steps,
            4,
            "a wrong-sized map is ignored"
        );
        let report = GenerationReport {
            generation: 0,
            best: 0.2,
            mean: 0.1,
            sd: 0.0,
            min: 0.0,
            best_value: 0.2,
            best_genome: cella_lib::explore::Genome(vec![ParamValue::Int(3)]),
            best_named: [("rule.subrules[0].count".to_string(), ParamValue::Int(3))]
                .into_iter()
                .collect(),
            evaluations: 8,
            invalid: 0,
            hall_of_fame_best: 0.2,
            archive: None,
            novelty_mean: None,
            archive_size: None,
        };
        handle_worker_msg(
            &mut app.explore,
            &mut app.view.layers,
            WorkerMsg::Generation(report.clone()),
        );
        assert_eq!(app.explore.fitness, vec![(0, 0.2, 0.1)]);
        assert_eq!(app.explore.best.as_ref().unwrap().0, 0.2);
        let worse = GenerationReport {
            generation: 1,
            best: 0.1,
            ..report
        };
        handle_worker_msg(
            &mut app.explore,
            &mut app.view.layers,
            WorkerMsg::Generation(worse),
        );
        assert_eq!(
            app.explore.best.as_ref().unwrap().0,
            0.2,
            "best only improves"
        );
        assert_eq!(app.explore.gens_done, 2);
        assert!(app.explore.worker.as_ref().unwrap().busy);
        handle_worker_msg(&mut app.explore, &mut app.view.layers, WorkerMsg::Done);
        assert!(!app.explore.worker.as_ref().unwrap().busy);
        handle_worker_msg(
            &mut app.explore,
            &mut app.view.layers,
            WorkerMsg::Error("boom".into()),
        );
        assert!(app.explore.message.as_deref().unwrap().contains("boom"));
        handle_worker_msg(
            &mut app.explore,
            &mut app.view.layers,
            WorkerMsg::Assimilated(AssimilationReport {
                scores: vec![1.0, 0.5],
                effective_sample_size: 1.6,
                parents: vec![0, 0],
                immigrants: 0,
                rejected: 0,
            }),
        );
        assert!(app.explore.last_report.is_some());
        // Replacing the grid drops the worker and the map but keeps the best genome.
        app.explore_on_grid_replaced();
        assert!(app.explore.worker.is_none());
        assert!(app.view.layers.probability_map.is_none());
        assert!(app.explore.best.is_some());
        // A message with no worker is ignored.
        handle_worker_msg(&mut app.explore, &mut app.view.layers, WorkerMsg::Done);
    }

    #[test]
    fn a_real_ensemble_worker_round_trips_and_stops() {
        let mut app = life_app();
        app.explore
            .genes
            .iter_mut()
            .find(|r| r.desc.key == "rule.subrules[2].count")
            .unwrap()
            .vary = true;
        app.explore.mc.members = 3;
        let cfg = app.current_ensemble_config();
        let (template, sig) = app.template_sim().unwrap();
        let worker = spawn_monte_carlo(template, cfg, app.tracked_types(), None, sig);
        // Construction reports a first map then Done.
        let first = worker.rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(matches!(first, WorkerMsg::Probability { steps: 0, .. }));
        assert!(matches!(
            worker.rx.recv_timeout(Duration::from_secs(10)).unwrap(),
            WorkerMsg::Done
        ));
        worker.tx.send(WorkerCmd::RunSteps(3)).unwrap();
        let mut steps_seen = 0;
        loop {
            match worker.rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                WorkerMsg::Probability { steps, cells } => {
                    steps_seen = steps;
                    assert_eq!(cells.len(), 1500);
                }
                WorkerMsg::Done => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(steps_seen, 3);
        let observed = observation_mask(&app.template_sim().unwrap().0, &app.explore.tracked);
        worker
            .tx
            .send(WorkerCmd::Assimilate {
                observed,
                types: app.tracked_types(),
            })
            .unwrap();
        let mut got_report = false;
        loop {
            match worker.rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                WorkerMsg::Assimilated(r) => {
                    assert_eq!(r.scores.len(), 3);
                    got_report = true;
                }
                WorkerMsg::Probability { .. } => {}
                WorkerMsg::Done => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(got_report);
        worker.tx.send(WorkerCmd::Stop).unwrap();
        worker.join.unwrap().join().unwrap();
        // An invalid config reports an error instead of panicking.
        let bad = EnsembleConfig {
            members: 0,
            ..EnsembleConfig::default()
        };
        let (template, sig) = app.template_sim().unwrap();
        let w = spawn_monte_carlo(template, bad, vec![], None, sig);
        assert!(matches!(
            w.rx.recv_timeout(Duration::from_secs(10)).unwrap(),
            WorkerMsg::Error(_)
        ));
    }

    #[test]
    fn a_real_evolution_worker_reports_generations() {
        let mut app = life_app();
        app.explore
            .genes
            .iter_mut()
            .find(|r| r.desc.key == "rule.subrules[2].count")
            .unwrap()
            .vary = true;
        app.explore.evo = EvoConfig {
            population: 4,
            steps: 5,
            repeats: 1,
            search: SearchChoice::MapElites,
            ..EvoConfig::default()
        };
        let cfg = app.current_evolve_config().unwrap();
        let (template, sig) = app.template_sim().unwrap();
        let worker = spawn_evolve(template, cfg, None, sig);
        assert!(matches!(
            worker.rx.recv_timeout(Duration::from_secs(10)).unwrap(),
            WorkerMsg::Done
        ));
        worker.tx.send(WorkerCmd::RunGenerations(2)).unwrap();
        let (mut gens, mut archives) = (0, 0);
        loop {
            match worker.rx.recv_timeout(Duration::from_secs(20)).unwrap() {
                WorkerMsg::Generation(r) => {
                    gens += 1;
                    assert!(r.archive.is_some());
                }
                WorkerMsg::Archive(snap) => {
                    archives += 1;
                    assert_eq!(snap.dims, vec![12, 12]);
                }
                WorkerMsg::Done => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(gens, 2);
        assert!(archives >= 1);
        worker.tx.send(WorkerCmd::Stop).unwrap();
        worker.join.unwrap().join().unwrap();
    }

    #[test]
    fn surprise_mutate_and_undo_rule_round_trip() {
        use crate::gui::actions::Action;
        let mut app = life_app();
        let before: Vec<(String, ParamValue)> = app.grid_knobs().1.into_iter().collect();
        // Surprise me changes at least one knob, leaves a valid grid, queues a fill.
        app.apply_action(Action::SurpriseMe { seed: 7 });
        let after = app.grid_knobs().1;
        assert!(
            before.iter().any(|(k, v)| after.get(k) != Some(v)),
            "no knob changed"
        );
        assert_eq!(app.edit.rule_undo.len(), 1);
        assert!(
            app.actions
                .iter()
                .any(|a| matches!(a, Action::RandomFill { seed: 7, .. }))
        );
        app.drain_actions();
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("Filled")
        );
        // Same seed, same surprise.
        let mut twin = life_app();
        twin.apply_action(Action::SurpriseMe { seed: 7 });
        twin.drain_actions();
        assert_eq!(twin.grid_knobs().1, app.grid_knobs().1);
        assert_eq!(
            twin.scenario.d2.as_ref().unwrap().cells(),
            app.scenario.d2.as_ref().unwrap().cells()
        );
        // Mutate nudges from the current values and stacks another undo.
        let mid = app.grid_knobs().1;
        app.apply_action(Action::MutateRule {
            seed: 3,
            sigma: 0.5,
        });
        assert_eq!(app.edit.rule_undo.len(), 2);
        assert_ne!(app.grid_knobs().1, mid);
        // Sigma 0 on a grid whose knobs sit inside their bounds changes nothing
        // and records nothing. (After a surprise a count can sit above its
        // neighbourhood's new size; a mutation then clamps it, which counts as
        // a change.)
        let mut calm = life_app();
        calm.apply_action(Action::MutateRule {
            seed: 3,
            sigma: 0.0,
        });
        assert!(calm.edit.rule_undo.is_empty());
        assert!(
            calm.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("unchanged")
        );
        // Undo walks back: first to the surprise, then to the original.
        app.apply_action(Action::UndoRule);
        assert_eq!(app.grid_knobs().1, mid);
        app.apply_action(Action::UndoRule);
        let restored = app.grid_knobs().1;
        for (k, v) in &before {
            assert_eq!(restored.get(k), Some(v), "{k}");
        }
        // The Reset snapshot mirrors the restored knobs.
        app.reset_to_initial();
        for (k, v) in &before {
            assert_eq!(app.grid_knobs().1.get(k), Some(v), "{k} after reset");
        }
        app.apply_action(Action::UndoRule);
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("Nothing")
        );
        // Playing refuses all three.
        app.playback.playing = true;
        app.apply_action(Action::SurpriseMe { seed: 1 });
        app.apply_action(Action::MutateRule {
            seed: 1,
            sigma: 0.5,
        });
        app.apply_action(Action::UndoRule);
        assert!(app.edit.rule_undo.is_empty() && app.actions.is_empty());
        app.playback.playing = false;
        // The stack is capped.
        for i in 0..(RULE_UNDO_CAP as u64 + 5) {
            app.apply_action(Action::MutateRule {
                seed: 100 + i,
                sigma: 0.9,
            });
        }
        assert!(app.edit.rule_undo.len() <= RULE_UNDO_CAP);
        // Keyboard forms read and advance the Edit tab's seed.
        app.edit.fill_seed = 40;
        app.apply_action(Action::RandomFillDraft);
        app.apply_action(Action::SurpriseMeDraft);
        app.apply_action(Action::MutateRuleDraft);
        assert_eq!(app.edit.fill_seed, 43);
        assert!(
            app.actions
                .iter()
                .any(|a| matches!(a, Action::SurpriseMe { seed: 41 }))
        );
        app.drain_actions();
        // No grid: everything degrades to a status.
        let mut empty = test_app();
        empty.apply_action(Action::SurpriseMe { seed: 1 });
        empty.apply_action(Action::MutateRule {
            seed: 1,
            sigma: 0.5,
        });
        assert!(
            empty
                .chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("Load")
        );
        assert!(empty.fill_type_or_default().is_none());
        // 1D: the Wolfram code is a knob too.
        let mut one = test_app();
        one.load_demo_1d_rule30();
        let code_before = one.grid_knobs().1;
        one.apply_action(Action::SurpriseMe { seed: 9 });
        one.drain_actions();
        assert_ne!(one.grid_knobs().1, code_before);
        one.apply_action(Action::UndoRule);
        assert_eq!(one.grid_knobs().1, code_before);
    }

    #[test]
    fn apply_genome_is_all_or_nothing_and_reaches_the_reset_snapshot() {
        let mut app = life_app();
        let ok = vec![("rule.subrules[0].count".to_string(), ParamValue::Int(5))];
        app.apply_genome(&ok).unwrap();
        assert_eq!(app.scenario.d2.as_ref().unwrap().rule.subrules[0].count, 5);
        app.reset_to_initial();
        assert_eq!(
            app.scenario.d2.as_ref().unwrap().rule.subrules[0].count,
            5,
            "Reset keeps the applied genome"
        );
        let bad = vec![
            ("rule.subrules[0].count".to_string(), ParamValue::Int(2)),
            ("rule.subrules[0].count".to_string(), ParamValue::Int(99)),
        ];
        let err = app.apply_genome(&bad).unwrap_err();
        assert!(err.contains("rule.subrules[0].count"), "{err}");
        assert_eq!(
            app.scenario.d2.as_ref().unwrap().rule.subrules[0].count,
            5,
            "nothing applied"
        );
        assert!(test_app().apply_genome(&ok).is_err());
    }

    #[test]
    fn resize_masks_remaps_nested_masks_and_leaves_wrong_length_arrays_alone() {
        let old_mask = vec![true, false, true, false];
        let mut v = json!({
            "objective": {
                "metric": "target_mask",
                "types": ["A"],
                "mask": old_mask,
            },
            "descriptors": [{"metric": "activity"}],
            // Named "mask" but the wrong length for the old grid: left as-is.
            "genes": [{"key": "p0", "mask": [true, true]}],
        });
        resize_masks(&mut v, (2, 2), (3, 2));
        let expected = cella_lib::resize::remap_blocks(&old_mask, 1, (2, 2), (3, 2), false);
        assert_eq!(v["objective"]["mask"], serde_json::to_value(&expected).unwrap());
        assert_eq!(v["objective"]["types"], json!(["A"]), "other fields untouched");
        assert_eq!(v["descriptors"][0]["metric"], "activity", "recursed into arrays");
        assert_eq!(
            v["genes"][0]["mask"],
            json!([true, true]),
            "a mask of a different length than the old grid is left alone"
        );
    }

    /// With no loaded ensemble/evolve block, `current_evolve_config` must
    /// build the "match the current grid" mask from the grid as it is right
    /// now, never from a value frozen at scenario load (FI1(a)): the base
    /// case is simply the panel's own config, unmerged.
    ///
    /// The scenario is reloaded *after* picking `MatchGrid` (as the final
    /// reviewer's repro did): that makes the objective genuinely "untouched"
    /// afterwards — its `metric` tag already matches `panel_at_load`'s, so
    /// only the mask differs. Before this fix, `without_masks` made the
    /// overlay's "did this field change?" check ignore exactly that
    /// difference, so it kept the load-time mask.
    #[test]
    fn current_evolve_config_with_no_base_uses_the_live_grids_mask_not_the_load_time_one() {
        let mut app = life_app();
        assert!(app.explore.base_evolve.is_none(), "life_app never loads a file");
        app.explore.objective.metric = MetricChoice::MatchGrid;
        app.load_demo_life(); // re-load with MatchGrid already selected
        app.reconcile_explore_state();
        let load_time_mask = observation_mask(&app.template_sim().unwrap().0, &app.explore.tracked);
        app.step_once(); // the Life demo's blinker rotates, moving its Alive cells
        let live_mask = observation_mask(&app.template_sim().unwrap().0, &app.explore.tracked);
        assert_ne!(
            load_time_mask, live_mask,
            "test setup: the step must actually move the blinker"
        );
        let cfg = app.current_evolve_config().unwrap();
        let Metric::TargetMask { mask, .. } = cfg.objective.unwrap().metric else {
            panic!("expected a target-mask objective");
        };
        assert_eq!(
            mask, live_mask,
            "a no-base config must read the CURRENT grid, not the load-time one"
        );
    }

    /// FI1(a): after a resize with no loaded block, both `current_evolve_config`
    /// and (once touched, here by an unrelated field) the saved block carry a
    /// mask sized for the new grid — reproducing the final reviewer's exact
    /// repro ("load a scenario, set the objective to MatchGrid, re-load,
    /// Resize"), where `mask.len()` used to stay at the old grid's cell count.
    #[test]
    fn current_evolve_config_mask_length_matches_the_grid_after_a_resize_with_no_base() {
        use crate::gui::actions::Action;
        let mut app = life_app();
        app.explore.objective.metric = MetricChoice::MatchGrid;
        app.load_demo_life(); // re-load with MatchGrid already selected
        app.reconcile_explore_state();
        app.apply_action(Action::Resize { w: 20, h: 12 });

        let cfg = app.current_evolve_config().unwrap();
        let Metric::TargetMask { mask, .. } = cfg.objective.clone().unwrap().metric else {
            panic!("expected a target-mask objective");
        };
        assert_eq!(
            mask.len(),
            20 * 12,
            "current_evolve_config's mask must fit the resized grid"
        );

        // Touch evolve on an unrelated field so the block is written, and
        // check the saved block's mask is sized for the new grid too (the
        // objective itself is still untouched, same as above).
        app.explore.evo.generations += 1;
        let saved = app.evolve_block_for_save().expect("the generations edit touched evolve");
        let Metric::TargetMask { mask, .. } = saved.objective.unwrap().metric else {
            panic!("expected a target-mask objective");
        };
        assert_eq!(
            mask.len(),
            20 * 12,
            "evolve_block_for_save's mask must fit the resized grid too"
        );
    }

    /// A small 2D config whose `evolve.objective` is a `target_mask` of
    /// `old_w`×`old_h`, for FI1(b): a loaded base's mask is a user's target,
    /// not the live grid's, and must survive a resize by being remapped, not
    /// replaced. The rule references "Alive" without ever changing anything,
    /// just so the type counts as declared (the GUI's tracked-type picker
    /// only offers declared types).
    fn write_target_mask_config(old_w: usize, old_h: usize, mask: &[bool]) -> std::path::PathBuf {
        let initial = vec!["Alive"; old_w * old_h];
        let cfg = json!({
            "dim": "2d",
            "width": old_w,
            "height": old_h,
            "history_limit": 0,
            "initial": initial,
            "rule": {"subrules": [{
                "current_type": "Alive",
                "criteria_type": "Alive",
                "count": 99,
                "op": "gt",
                "range": 1,
                "neighborhood": "Moore",
                "randomness": null,
                "output_type": "Alive",
            }]},
            "evolve": {
                "objective": {
                    "metric": "target_mask",
                    "types": ["Alive"],
                    "mask": mask,
                },
            },
        });
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cella_gui_target_mask_base_{}_{stamp}.json",
            std::process::id()
        ));
        std::fs::write(&path, serde_json::to_string(&cfg).unwrap()).unwrap();
        path
    }

    #[test]
    fn a_loaded_target_mask_base_is_resized_top_left_not_replaced_by_the_live_grid() {
        use crate::gui::actions::Action;
        let (old_w, old_h) = (5, 4);
        // Every cell in the actual grid is "Alive", so a mask that matched
        // the live grid would be all-true; this checkerboard is not, which
        // lets the last assertion tell "resized" apart from "replaced".
        let old_mask: Vec<bool> = (0..old_w * old_h).map(|i| i % 2 == 0).collect();
        let path = write_target_mask_config(old_w, old_h, &old_mask);
        let mut app = test_app();
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(app.explore.base_evolve.is_some(), "the file's evolve block should load");

        let (new_w, new_h) = (8, 6); // grow
        app.apply_action(Action::Resize { w: new_w, h: new_h });

        let base = app.explore.base_evolve.as_ref().expect("still present after resize");
        let Metric::TargetMask { mask: base_mask, .. } = base.objective.as_ref().unwrap().metric.clone()
        else {
            panic!("expected a target-mask objective");
        };
        assert_eq!(base_mask.len(), new_w * new_h, "new length");
        let expected = cella_lib::resize::remap_blocks(&old_mask, 1, (old_w, old_h), (new_w, new_h), false);
        assert_eq!(
            base_mask, expected,
            "top-left overlap keeps the old values, new cells are false"
        );
        for y in 0..old_h {
            for x in 0..old_w {
                assert_eq!(
                    base_mask[y * new_w + x],
                    old_mask[y * old_w + x],
                    "({x},{y}) in the overlap"
                );
            }
        }
        for y in 0..new_h {
            for x in old_w..new_w {
                assert!(!base_mask[y * new_w + x], "new column ({x},{y}) is false");
            }
        }
        for y in old_h..new_h {
            for x in 0..new_w {
                assert!(!base_mask[y * new_w + x], "new row ({x},{y}) is false");
            }
        }

        // Prove it was resized, not replaced: it must still differ from the
        // live grid's own mask of the same tracked type (every live cell is
        // "Alive", so its mask is all-true in the old area and all-false in
        // the new one — nothing like the checkerboard above).
        let live_mask = observation_mask(&app.template_sim().unwrap().0, &app.explore.tracked);
        assert_ne!(base_mask, live_mask);

        // The load-time baseline's evolve JSON is remapped the same way. Its
        // mask started as the panel's own approximation of "match the
        // current grid" (the live grid's mask at load time, all-true here —
        // every cell starts "Alive"), not the file's checkerboard, so its
        // remapped shape matches the resized live grid's mask, not `base_mask`.
        let baseline_mask = app
            .explore
            .panel_at_load
            .as_ref()
            .and_then(|b| b.evolve.as_ref())
            .and_then(|v| v["objective"]["mask"].as_array())
            .map(|a| a.iter().map(|b| b.as_bool().unwrap()).collect::<Vec<_>>())
            .expect("the baseline's evolve JSON has an objective mask");
        assert_eq!(baseline_mask, live_mask);
    }
}
