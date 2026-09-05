//! How a run is measured and scored.
//!
//! Three layers, each built on the one below:
//!
//! 1. **Plain measurements** — free functions such as [`fraction`],
//!    [`activity`], [`entropy`] and [`iou`]. Give them a grid (or two masks)
//!    and they return a number. Use these directly when you write your own
//!    scoring code.
//! 2. **[`Metric`]** — the same measurements as data, so a config file can
//!    name one: `{"metric": "fraction", "types": ["Alive"]}`. A metric knows
//!    how to `sample` a grid, whether it needs a sample after *every* step or
//!    only at the end, and how to turn the samples of one run into one value.
//! 3. **[`Objective`]** — a metric plus *when* to look ([`When`]) and *what
//!    counts as good* ([`Goal`]). This is what an evolution maximises. It
//!    implements [`Fitness`], the small trait the engines actually call, so a
//!    Rust caller can substitute any scoring code of their own.
//!
//! The sampling protocol every [`Fitness`] follows: one sample **before the
//! first step** (index 0), then either one sample after every step (when
//! [`Fitness::every_step`] is true) or a single sample at the end. So a run of
//! `steps` steps yields `steps + 1` samples in the first case and 2 in the
//! second. [`Fitness::aggregate`] turns the list into the raw value a report
//! shows; [`Fitness::score`] is the number the engines maximise.

use lasso2::Key;
use serde::{Deserialize, Serialize};

use super::sim::Sim;
use crate::external::ModelError;
use crate::types::CellType;

// ---------------------------------------------------------------------------
// Plain measurements
// ---------------------------------------------------------------------------

/// Intersection over union of two masks (1.0 when both are empty).
pub fn iou(a: &[bool], b: &[bool]) -> f64 {
    let (mut inter, mut union) = (0u64, 0u64);
    for (&x, &y) in a.iter().zip(b) {
        inter += (x && y) as u64;
        union += (x || y) as u64;
    }
    if union == 0 {
        1.0
    } else {
        inter as f64 / union as f64
    }
}

/// Sørensen–Dice overlap of two masks: `2|A∩B| / (|A| + |B|)` (1.0 when both
/// are empty). Kinder to small shifts than IoU.
pub fn sorensen(a: &[bool], b: &[bool]) -> f64 {
    let (mut inter, mut total) = (0u64, 0u64);
    for (&x, &y) in a.iter().zip(b) {
        inter += (x && y) as u64;
        total += x as u64 + y as u64;
    }
    if total == 0 {
        1.0
    } else {
        2.0 * inter as f64 / total as f64
    }
}

/// Fraction of positions where two masks agree (1.0 for empty inputs).
pub fn agreement(a: &[bool], b: &[bool]) -> f64 {
    if a.is_empty() {
        return 1.0;
    }
    let same = a.iter().zip(b).filter(|(x, y)| x == y).count();
    same as f64 / a.len() as f64
}

/// Brier score of a probability map against what happened: the mean squared
/// gap between each probability and 0/1. Lower is better; 0 is perfect.
pub fn brier(prob: &[f32], obs: &[bool]) -> f64 {
    if prob.is_empty() {
        return 0.0;
    }
    let sum: f64 = prob
        .iter()
        .zip(obs)
        .map(|(&p, &o)| {
            let d = f64::from(p) - if o { 1.0 } else { 0.0 };
            d * d
        })
        .sum();
    sum / prob.len() as f64
}

/// Share of cells that are in any of `types` (0 for an empty grid).
pub fn fraction(sim: &Sim, types: &[CellType]) -> f64 {
    if sim.is_empty() {
        return 0.0;
    }
    let n: u64 = types
        .iter()
        .map(|t| sim.counts().get(&t.0).copied().unwrap_or(0))
        .sum();
    n as f64 / sim.len() as f64
}

/// Share of cells that changed on the last step. Before the first step every
/// cell counts as "just changed", so this reads 1.0 at step 0.
pub fn activity(sim: &Sim) -> f64 {
    if sim.is_empty() {
        return 0.0;
    }
    let changed = sim.ages().iter().filter(|a| **a == 0).count();
    changed as f64 / sim.len() as f64
}

/// Shannon entropy, in bits, of the distribution of types over the grid.
/// 0 when every cell is the same type; `log2(k)` when `k` types are equally
/// common.
pub fn entropy(sim: &Sim) -> f64 {
    let total = sim.len() as f64;
    if total == 0.0 {
        return 0.0;
    }
    sim.counts()
        .values()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / total;
            -p * p.log2()
        })
        .sum()
}

/// Mean and (population) standard deviation of a list; `(0, 0)` when empty.
pub fn mean_sd(v: &[f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    (mean, var.sqrt())
}

/// Bounding box of the cells in `types` as a fraction of the grid area
/// (0 when none are present).
pub fn bbox_fraction(sim: &Sim, types: &[CellType]) -> f64 {
    let (w, h) = sim.dims();
    if w == 0 || h == 0 {
        return 0.0;
    }
    let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0usize, usize::MAX, 0usize);
    let mut any = false;
    for (i, c) in sim.cells().iter().enumerate() {
        if types.contains(c) {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            any = true;
        }
    }
    if !any {
        return 0.0;
    }
    ((x1 - x0 + 1) * (y1 - y0 + 1)) as f64 / (w * h) as f64
}

/// Largest elongation reported; a single row of cells would be infinite.
pub const MAX_ELONGATION: f64 = 10.0;

/// How stretched the set of tracked cells is: the square root of the ratio
/// of the two eigenvalues of its second-moment matrix (the same measure the
/// wildfire validation calls "elongation", experiment E12). 1 means as wide
/// as it is long in every direction (a disc, a square); 2 means twice as
/// long as wide, whichever way it points. Clamped to `[1, MAX_ELONGATION]`;
/// 1 when fewer than two cells are tracked.
pub fn elongation(sim: &Sim, types: &[CellType]) -> f64 {
    let (w, _) = sim.dims();
    if w == 0 {
        return 1.0;
    }
    let (mut n, mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    for (i, c) in sim.cells().iter().enumerate() {
        if types.contains(c) {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            n += 1.0;
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
        }
    }
    if n < 2.0 {
        return 1.0;
    }
    let (mx, my) = (sx / n, sy / n);
    // Add a twelfth per axis: each cell is a unit square, not a point, so a
    // straight line of cells has a finite width.
    let cxx = sxx / n - mx * mx + 1.0 / 12.0;
    let cyy = syy / n - my * my + 1.0 / 12.0;
    let cxy = sxy / n - mx * my;
    let tr = cxx + cyy;
    let det = (cxx * cyy - cxy * cxy).max(0.0);
    let disc = (tr * tr / 4.0 - det).max(0.0).sqrt();
    let (l1, l2) = (tr / 2.0 + disc, (tr / 2.0 - disc).max(1e-12));
    (l1 / l2).sqrt().clamp(1.0, MAX_ELONGATION)
}

/// Centre of mass `(x, y)` of the cells in `types`, or `None` if there are none.
pub fn centroid(cells: &[CellType], width: usize, types: &[CellType]) -> Option<(f64, f64)> {
    if width == 0 {
        return None;
    }
    let (mut sx, mut sy, mut n) = (0.0f64, 0.0f64, 0u64);
    for (i, c) in cells.iter().enumerate() {
        if types.contains(c) {
            sx += (i % width) as f64;
            sy += (i / width) as f64;
            n += 1;
        }
    }
    (n > 0).then(|| (sx / n as f64, sy / n as f64))
}

/// How far the centre of mass of `types` moved on the last step, in cells.
/// 0 before the first step or when either state has no such cells.
pub fn centroid_speed(sim: &Sim, types: &[CellType]) -> f64 {
    if sim.step_count() == 0 {
        return 0.0;
    }
    let w = sim.width();
    match (
        centroid(sim.cells(), w, types),
        centroid(sim.prev_cells(), w, types),
    ) {
        (Some((x1, y1)), Some((x0, y0))) => ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt(),
        _ => 0.0,
    }
}

/// A 64-bit fingerprint of the whole grid. Two grids with the same cells hash
/// the same; that is all a period detector needs.
pub(crate) fn state_hash(sim: &Sim) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in sim.cells() {
        h ^= c.0.into_usize() as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// Smallest period `p` (1..=`window/2`) such that the last `window` states
/// repeat every `p` steps, or 0 if none does. `states` are per-step
/// fingerprints, oldest first.
fn detect_period(states: &[f64], window: usize) -> u32 {
    let tail = &states[states.len().saturating_sub(window)..];
    let n = tail.len();
    for p in 1..=n / 2 {
        if (p..n).all(|t| tail[t] == tail[t - p]) {
            return p as u32;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Metric / Objective
// ---------------------------------------------------------------------------

/// How a `target_mask` metric compares the simulated mask with the target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskScore {
    /// Intersection over union ([`iou`]).
    #[default]
    Iou,
    /// Sørensen–Dice ([`sorensen`]).
    Sorensen,
    /// Share of cells that agree ([`agreement`]).
    Agreement,
}

/// A measurement of one run, as data so a config file can ask for it.
///
/// `types` lists cell-type names; the metric treats a cell as "on" when it
/// is in any of them. Metrics marked *descriptor* can also serve as the axes
/// of an archive (see [`Metric::is_descriptor`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "metric", rename_all = "snake_case")]
pub enum Metric {
    /// Share of cells in `types`. Descriptor, range 0..1.
    Fraction { types: Vec<String> },
    /// Share of cells that changed on the last step. Descriptor, range 0..1.
    Activity,
    /// Shannon entropy (bits) of the type histogram. Descriptor, range
    /// 0..log2(number of types).
    Entropy,
    /// Steps until nothing changes any more, capped at the run length.
    /// Descriptor, range 0..steps. `when` is ignored.
    Lifetime,
    /// Compare the mask of `types` with a target mask (one flag per cell).
    TargetMask {
        types: Vec<String>,
        mask: Vec<bool>,
        #[serde(default)]
        score: MaskScore,
    },
    /// Root-mean-square error between the fraction of `types` after each step
    /// and `target[step]` (shorter list wins). Use with `goal: minimise`.
    Series {
        types: Vec<String>,
        target: Vec<f64>,
    },
    /// The classic density-classification task on two types `[a, b]`: a run
    /// is solved when it ends with every cell equal to whichever type was in
    /// the majority at the start. Value 1 (solved) or 0. The evolution engine
    /// draws a random start for each repeat when this metric is used.
    DensityClassification { types: [String; 2] },
    /// Bounding-box area of `types` as a fraction of the grid. Descriptor,
    /// range 0..1.
    BboxFraction { types: Vec<String> },
    /// How stretched the tracked cells are: `√(λ₁/λ₂)` of their
    /// second-moment (covariance) matrix, 1 for a disc or square, larger
    /// for a streak, whatever its direction; clamped to `[1, 10]`, 1 when
    /// fewer than two cells are tracked.
    Elongation { types: Vec<String> },
    /// Mean per-step movement of the centre of mass of `types`, in cells.
    /// Descriptor, range 0..1 (a pattern cannot move faster than one cell a
    /// step; noisy small masses can, and are clamped).
    CentroidSpeed { types: Vec<String> },
    /// Final fraction of `types` minus the starting fraction. Descriptor,
    /// range -1..1.
    Growth { types: Vec<String> },
    /// Cycle length of the grid over the last `window` steps (0 = no cycle
    /// found). Descriptor, range 0..window/2. A still life has period 1, a
    /// blinker 2, chaos 0.
    Period {
        #[serde(default = "default_window")]
        window: u32,
    },
}

fn default_window() -> u32 {
    64
}

fn cell_types(names: &[String]) -> Vec<CellType> {
    names.iter().map(|n| CellType::new(n)).collect()
}

impl Metric {
    /// Type names this metric mentions, for validation against a grid.
    pub fn type_names(&self) -> Vec<&str> {
        match self {
            Metric::Fraction { types }
            | Metric::TargetMask { types, .. }
            | Metric::Series { types, .. }
            | Metric::BboxFraction { types }
            | Metric::Elongation { types }
            | Metric::CentroidSpeed { types }
            | Metric::Growth { types } => types.iter().map(String::as_str).collect(),
            Metric::DensityClassification { types } => types.iter().map(String::as_str).collect(),
            Metric::Activity | Metric::Entropy | Metric::Lifetime | Metric::Period { .. } => {
                Vec::new()
            }
        }
    }

    /// Check the metric makes sense for `sim`: every type it names must be
    /// one the grid declares, and a target mask must have one entry per cell.
    pub fn validate(&self, sim: &Sim) -> Result<(), ModelError> {
        let declared = sim.declared_types();
        for name in self.type_names() {
            if !declared.iter().any(|t| t.as_str() == name) {
                let known: Vec<&str> = declared.iter().map(|t| t.as_str()).collect();
                return Err(ModelError::InvalidParam(format!(
                    "metric names type '{name}', which this grid does not declare (declared: {})",
                    known.join(", ")
                )));
            }
        }
        if let Metric::TargetMask { mask, .. } = self
            && mask.len() != sim.len()
        {
            return Err(ModelError::LayerLength {
                layer: "target mask",
                expected: sim.len(),
                got: mask.len(),
            });
        }
        if let Metric::Series { target, .. } = self
            && target.is_empty()
        {
            return Err(ModelError::InvalidParam(
                "series metric needs at least one target value".into(),
            ));
        }
        if let Metric::Period { window } = self
            && *window < 2
        {
            return Err(ModelError::InvalidParam(
                "period metric needs a window of at least 2 steps".into(),
            ));
        }
        Ok(())
    }

    /// Whether this metric can be an archive axis. Target-relative metrics
    /// (mask, series, the classification task) cannot: they describe how well
    /// a run matched something, not what the run *did*.
    pub fn is_descriptor(&self) -> bool {
        !matches!(
            self,
            Metric::TargetMask { .. }
                | Metric::Series { .. }
                | Metric::DensityClassification { .. }
        )
    }

    /// The natural `[low, high]` range of this metric's value, used as the
    /// default extent of an archive axis.
    pub fn range(&self, sim: &Sim, steps: u64) -> [f64; 2] {
        match self {
            Metric::Fraction { .. }
            | Metric::Activity
            | Metric::BboxFraction { .. }
            | Metric::CentroidSpeed { .. }
            | Metric::TargetMask { .. }
            | Metric::Series { .. }
            | Metric::DensityClassification { .. } => [0.0, 1.0],
            Metric::Entropy => [0.0, (sim.declared_types().len().max(2) as f64).log2()],
            Metric::Lifetime => [0.0, steps as f64],
            Metric::Growth { .. } => [-1.0, 1.0],
            Metric::Elongation { .. } => [1.0, MAX_ELONGATION],
            Metric::Period { window } => [0.0, f64::from(*window / 2)],
        }
    }

    /// One reading of the metric on the grid as it is now.
    pub fn sample(&self, sim: &Sim) -> f64 {
        match self {
            Metric::Fraction { types }
            | Metric::Series { types, .. }
            | Metric::Growth { types } => fraction(sim, &cell_types(types)),
            Metric::Activity | Metric::Lifetime => activity(sim),
            Metric::Entropy => entropy(sim),
            Metric::TargetMask { types, mask, score } => {
                let got = sim.mask(&cell_types(types));
                match score {
                    MaskScore::Iou => iou(&got, mask),
                    MaskScore::Sorensen => sorensen(&got, mask),
                    MaskScore::Agreement => agreement(&got, mask),
                }
            }
            Metric::DensityClassification { types } => fraction(sim, &[CellType::new(&types[0])]),
            Metric::BboxFraction { types } => bbox_fraction(sim, &cell_types(types)),
            Metric::Elongation { types } => elongation(sim, &cell_types(types)),
            Metric::CentroidSpeed { types } => centroid_speed(sim, &cell_types(types)),
            // Keep 52 bits so the fingerprint survives the trip through f64.
            Metric::Period { .. } => (state_hash(sim) >> 12) as f64,
        }
    }

    /// Whether this metric must see the grid after every step.
    pub fn needs_every_step(&self) -> bool {
        matches!(
            self,
            Metric::Lifetime
                | Metric::Series { .. }
                | Metric::CentroidSpeed { .. }
                | Metric::Period { .. }
        )
    }

    /// Turn the samples of one run into the metric's value, honouring `when`
    /// for the metrics where "when" makes sense.
    pub fn aggregate(&self, samples: &[f64], when: When) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        match self {
            Metric::Lifetime => {
                // samples[0] is the pre-step reading (always "changed"); the
                // lifetime is the number of steps that changed something
                // before the first step that changed nothing.
                samples
                    .iter()
                    .skip(1)
                    .position(|a| *a == 0.0)
                    .unwrap_or(samples.len() - 1) as f64
            }
            Metric::Series { target, .. } => {
                let pairs: Vec<(f64, f64)> = samples
                    .iter()
                    .skip(1)
                    .zip(target)
                    .map(|(a, b)| (*a, *b))
                    .collect();
                if pairs.is_empty() {
                    return 0.0;
                }
                let mse =
                    pairs.iter().map(|(a, b)| (a - b) * (a - b)).sum::<f64>() / pairs.len() as f64;
                mse.sqrt()
            }
            Metric::DensityClassification { .. } => {
                let start = samples[0];
                let end = samples[samples.len() - 1];
                let solved = (start > 0.5 && end == 1.0) || (start < 0.5 && end == 0.0);
                if solved { 1.0 } else { 0.0 }
            }
            Metric::Growth { .. } => {
                let end = match when {
                    When::Step(k) => samples[(k as usize).min(samples.len() - 1)],
                    _ => samples[samples.len() - 1],
                };
                end - samples[0]
            }
            Metric::CentroidSpeed { .. } => {
                let (mean, _) = mean_sd(&samples[1.min(samples.len() - 1)..]);
                mean.min(1.0)
            }
            Metric::Period { window } => {
                let end = match when {
                    When::Step(k) => (k as usize).min(samples.len() - 1),
                    _ => samples.len() - 1,
                };
                f64::from(detect_period(&samples[..=end], *window as usize))
            }
            _ => match when {
                When::End => samples[samples.len() - 1],
                When::Step(k) => samples[(k as usize).min(samples.len() - 1)],
                When::Mean => mean_sd(&samples[1.min(samples.len() - 1)..]).0,
            },
        }
    }
}

/// Which reading of a metric counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    /// The reading after the last step.
    #[default]
    End,
    /// The reading after step `k` (clamped to the run length).
    Step(u64),
    /// The average over every step.
    Mean,
}

/// What counts as good.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    /// Bigger is better.
    #[default]
    #[serde(alias = "maximize")]
    Maximise,
    /// Smaller is better.
    #[serde(alias = "minimize")]
    Minimise,
    /// Closest to this value is best; the score is `-|value - target|`, so
    /// 0 is perfect.
    Target(f64),
}

impl Goal {
    /// Map a raw value onto "higher is better".
    pub fn score(self, value: f64) -> f64 {
        match self {
            Goal::Maximise => value,
            Goal::Minimise => -value,
            Goal::Target(t) => -(value - t).abs(),
        }
    }
}

/// A metric, when to read it, and what counts as good — the thing an
/// evolution maximises.
///
/// In JSON the metric's fields sit next to `goal` and `when`:
/// `{"metric": "fraction", "types": ["Alive"], "when": "end", "goal": "maximise"}`.
/// (Because the metric is flattened in, misspelled extra fields here are
/// ignored rather than refused — check the `metric` name and its own fields
/// carefully.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Objective {
    #[serde(flatten)]
    pub metric: Metric,
    #[serde(default)]
    pub goal: Goal,
    #[serde(default)]
    pub when: When,
}

impl Objective {
    /// An objective that maximises `metric` at the end of the run.
    pub fn maximise(metric: Metric) -> Self {
        Objective {
            metric,
            goal: Goal::Maximise,
            when: When::End,
        }
    }

    /// Check the objective against a grid (see [`Metric::validate`]).
    pub fn validate(&self, sim: &Sim) -> Result<(), ModelError> {
        self.metric.validate(sim)
    }
}

/// What the engines call to score a run. Implemented by [`Objective`]; a Rust
/// caller can implement it on their own type to score in any way they like.
///
/// See the module docs for the sampling protocol: `sample` is called before
/// the first step, then after every step if `every_step` is true or once at
/// the end if not; `aggregate` reduces the samples to the value shown in
/// reports; `score` is what is maximised.
pub trait Fitness: Send + Sync {
    /// One reading of the grid as it is now.
    fn sample(&self, sim: &Sim) -> f64;
    /// Whether a reading is needed after every step (default: only at the end).
    fn every_step(&self) -> bool {
        false
    }
    /// The raw value of a run, from its samples.
    fn aggregate(&self, samples: &[f64]) -> f64;
    /// The number to maximise (default: the raw value).
    fn score(&self, samples: &[f64]) -> f64 {
        self.aggregate(samples)
    }
}

impl Fitness for Objective {
    fn sample(&self, sim: &Sim) -> f64 {
        self.metric.sample(sim)
    }

    fn every_step(&self) -> bool {
        self.metric.needs_every_step() || !matches!(self.when, When::End)
    }

    fn aggregate(&self, samples: &[f64]) -> f64 {
        self.metric.aggregate(samples, self.when)
    }

    fn score(&self, samples: &[f64]) -> f64 {
        self.goal.score(self.aggregate(samples))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid1d::Grid1D;
    use crate::grid2d::Grid2D;
    use crate::rules::{CountOp, Neighborhood2D, Rule1D, Rule1DSubrule, Rule2D, Rule2DSubrule};

    fn life_rule() -> Rule2D {
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
        Rule2D {
            subrules: vec![
                sub(alive, 4, CountOp::Gt, None, dead),
                sub(alive, 2, CountOp::Gt, Some(3), alive),
                sub(dead, 3, CountOp::Eq, None, alive),
            ],
        }
    }

    fn life(w: usize, h: usize, on: &[(usize, usize)]) -> Sim {
        let alive = CellType::from("Alive");
        let mut cells = vec![CellType::inactive(); w * h];
        for (x, y) in on {
            cells[y * w + x] = alive;
        }
        Sim::D2(Grid2D::new(w, h, 0, cells, life_rule()))
    }

    fn blinker() -> Sim {
        life(5, 5, &[(1, 2), (2, 2), (3, 2)])
    }

    fn block() -> Sim {
        life(4, 4, &[(1, 1), (2, 1), (1, 2), (2, 2)])
    }

    fn glider() -> Sim {
        life(12, 12, &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)])
    }

    fn rule30(width: usize) -> Sim {
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

    fn run(sim: &mut Sim, f: &dyn Fitness, steps: u64) -> Vec<f64> {
        let mut samples = vec![f.sample(sim)];
        for _ in 0..steps {
            sim.step();
            if f.every_step() {
                samples.push(f.sample(sim));
            }
        }
        if !f.every_step() {
            samples.push(f.sample(sim));
        }
        samples
    }

    #[test]
    fn mask_metrics_have_the_textbook_values() {
        let a = [true, true, false, false];
        let b = [true, false, true, false];
        assert!((iou(&a, &b) - 1.0 / 3.0).abs() < 1e-12);
        assert!((sorensen(&a, &b) - 0.5).abs() < 1e-12);
        assert!((agreement(&a, &b) - 0.5).abs() < 1e-12);
        assert_eq!(iou(&[false, false], &[false, false]), 1.0);
        assert_eq!(sorensen(&[false], &[false]), 1.0);
        assert_eq!(agreement(&[], &[]), 1.0);
        assert!((brier(&[1.0, 0.0, 0.5], &[true, false, true]) - 0.25 / 3.0).abs() < 1e-12);
        assert_eq!(brier(&[], &[]), 0.0);
        assert_eq!(mean_sd(&[]), (0.0, 0.0));
        let (m, s) = mean_sd(&[2.0, 4.0]);
        assert_eq!((m, s), (3.0, 1.0));
    }

    #[test]
    fn grid_measurements_on_hand_built_grids() {
        let alive = CellType::from("Alive");
        let mut b = blinker();
        assert!((fraction(&b, &[alive]) - 3.0 / 25.0).abs() < 1e-12);
        assert_eq!(activity(&b), 1.0, "everything counts as changed at step 0");
        let p: f64 = 3.0 / 25.0;
        let expect = -(p * p.log2() + (1.0 - p) * (1.0 - p).log2());
        assert!((entropy(&b) - expect).abs() < 1e-12);
        assert!((bbox_fraction(&b, &[alive]) - 3.0 / 25.0).abs() < 1e-12);
        assert_eq!(bbox_fraction(&b, &[CellType::from("Nope")]), 0.0);
        assert_eq!(centroid(b.cells(), 5, &[alive]), Some((2.0, 2.0)));
        assert_eq!(centroid(b.cells(), 5, &[CellType::from("Nope")]), None);
        assert_eq!(centroid_speed(&b, &[alive]), 0.0, "no previous state yet");
        b.step();
        assert!((activity(&b) - 4.0 / 25.0).abs() < 1e-12);
        assert!(
            (bbox_fraction(&b, &[alive]) - 3.0 / 25.0).abs() < 1e-12,
            "now vertical"
        );
        assert_eq!(centroid_speed(&b, &[alive]), 0.0, "a blinker does not move");
        let empty = Sim::D2(Grid2D::new(0, 0, 0, vec![], life_rule()));
        assert_eq!(fraction(&empty, &[alive]), 0.0);
        assert_eq!(activity(&empty), 0.0);
        assert_eq!(entropy(&empty), 0.0);
        assert_eq!(bbox_fraction(&empty, &[alive]), 0.0);
        assert_eq!(centroid(&[], 0, &[alive]), None);
    }

    #[test]
    fn a_glider_moves_and_has_period_four_in_shape() {
        let alive = CellType::from("Alive");
        let mut g = glider();
        let obj = Objective::maximise(Metric::CentroidSpeed {
            types: vec!["Alive".into()],
        });
        let samples = run(&mut g, &obj, 8);
        let speed = obj.aggregate(&samples);
        // A glider covers one diagonal cell every four steps; its centre of
        // mass wobbles, so the mean per-step speed is around 0.3-0.5 cells.
        assert!(speed > 0.2 && speed <= 1.0, "speed {speed}");
        assert!(fraction(&g, &[alive]) > 0.0);
    }

    #[test]
    fn period_detects_still_life_blinker_and_chaos() {
        let period = Metric::Period { window: 16 };
        let mut b = block();
        let s = run(&mut b, &Objective::maximise(period.clone()), 10);
        assert_eq!(
            period.aggregate(&s, When::End),
            1.0,
            "a block never changes"
        );
        let mut bl = blinker();
        let s = run(&mut bl, &Objective::maximise(period.clone()), 10);
        assert_eq!(period.aggregate(&s, When::End), 2.0);
        let mut r = rule30(65);
        let s = run(&mut r, &Objective::maximise(period.clone()), 30);
        assert_eq!(
            period.aggregate(&s, When::End),
            0.0,
            "rule 30 does not cycle in 16 steps"
        );
        assert_eq!(detect_period(&[], 8), 0);
        assert_eq!(detect_period(&[1.0], 8), 0);
        // `Step(k)` looks at the window ending at step k.
        assert_eq!(period.aggregate(&s, When::Step(0)), 0.0);
    }

    #[test]
    fn lifetime_growth_and_series_read_the_sample_list() {
        let lifetime = Metric::Lifetime;
        // Changed for 3 steps, then quiet.
        assert_eq!(
            lifetime.aggregate(&[1.0, 0.2, 0.1, 0.05, 0.0, 0.0], When::End),
            3.0
        );
        assert_eq!(
            lifetime.aggregate(&[1.0, 0.2, 0.1], When::End),
            2.0,
            "never quiet: run length"
        );
        assert_eq!(lifetime.aggregate(&[], When::End), 0.0);
        let growth = Metric::Growth {
            types: vec!["Alive".into()],
        };
        assert!((growth.aggregate(&[0.1, 0.2, 0.4], When::End) - 0.3).abs() < 1e-12);
        assert!((growth.aggregate(&[0.1, 0.2, 0.4], When::Step(1)) - 0.1).abs() < 1e-12);
        let series = Metric::Series {
            types: vec!["Alive".into()],
            target: vec![0.5, 0.5],
        };
        assert!(
            (series.aggregate(&[0.0, 0.5, 0.7], When::End) - (0.04f64 / 2.0).sqrt()).abs() < 1e-12
        );
        assert_eq!(
            series.aggregate(&[0.3], When::End),
            0.0,
            "no post-step samples"
        );
        let frac = Metric::Fraction {
            types: vec!["Alive".into()],
        };
        assert_eq!(frac.aggregate(&[0.1, 0.2, 0.6], When::End), 0.6);
        assert_eq!(frac.aggregate(&[0.1, 0.2, 0.6], When::Step(1)), 0.2);
        assert_eq!(
            frac.aggregate(&[0.1, 0.2, 0.6], When::Step(99)),
            0.6,
            "clamped"
        );
        assert!((frac.aggregate(&[0.1, 0.2, 0.6], When::Mean) - 0.4).abs() < 1e-12);
        assert_eq!(frac.aggregate(&[0.7], When::Mean), 0.7);
    }

    #[test]
    fn density_classification_is_solved_only_by_the_right_uniform_end() {
        let dc = Metric::DensityClassification {
            types: ["X".into(), "Inactive".into()],
        };
        assert_eq!(dc.aggregate(&[0.7, 1.0], When::End), 1.0);
        assert_eq!(dc.aggregate(&[0.3, 0.0], When::End), 1.0);
        assert_eq!(dc.aggregate(&[0.7, 0.0], When::End), 0.0);
        assert_eq!(dc.aggregate(&[0.3, 0.4], When::End), 0.0);
        assert_eq!(
            dc.aggregate(&[0.5, 1.0], When::End),
            0.0,
            "a tie has no majority"
        );
        assert!(!dc.is_descriptor());
        assert!(
            !Metric::Series {
                types: vec![],
                target: vec![1.0]
            }
            .is_descriptor()
        );
        assert!(
            !Metric::TargetMask {
                types: vec![],
                mask: vec![],
                score: MaskScore::Iou
            }
            .is_descriptor()
        );
        assert!(Metric::Activity.is_descriptor());
        assert!(Metric::Period { window: 8 }.is_descriptor());
    }

    #[test]
    fn target_mask_uses_the_chosen_score_and_goals_flip_the_sign() {
        let b = blinker();
        let alive = CellType::from("Alive");
        let mut target = b.mask(&[alive]);
        target[0] = true; // one false alarm in the target
        let mk = |score| Metric::TargetMask {
            types: vec!["Alive".into()],
            mask: target.clone(),
            score,
        };
        assert!((mk(MaskScore::Iou).sample(&b) - 0.75).abs() < 1e-12);
        assert!((mk(MaskScore::Sorensen).sample(&b) - 6.0 / 7.0).abs() < 1e-12);
        assert!((mk(MaskScore::Agreement).sample(&b) - 24.0 / 25.0).abs() < 1e-12);
        assert_eq!(Goal::Maximise.score(0.3), 0.3);
        assert_eq!(Goal::Minimise.score(0.3), -0.3);
        assert!((Goal::Target(0.5).score(0.3) + 0.2).abs() < 1e-12);
        let obj = Objective {
            metric: Metric::Fraction {
                types: vec!["Alive".into()],
            },
            goal: Goal::Target(0.5),
            when: When::End,
        };
        assert!((obj.score(&[0.0, 0.3]) + 0.2).abs() < 1e-12);
        assert_eq!(obj.aggregate(&[0.0, 0.3]), 0.3);
        assert!(!Objective::maximise(Metric::Activity).every_step());
        assert!(
            Objective {
                when: When::Mean,
                ..Objective::maximise(Metric::Activity)
            }
            .every_step()
        );
        assert!(Objective::maximise(Metric::Lifetime).every_step());
    }

    #[test]
    fn validate_checks_types_masks_series_and_windows() {
        let b = blinker();
        assert!(
            Metric::Fraction {
                types: vec!["Alive".into()]
            }
            .validate(&b)
            .is_ok()
        );
        let err = Metric::Fraction {
            types: vec!["Ghost".into()],
        }
        .validate(&b)
        .unwrap_err();
        assert!(format!("{err}").contains("Ghost"), "{err}");
        assert!(
            Metric::TargetMask {
                types: vec!["Alive".into()],
                mask: vec![true; 25],
                score: MaskScore::Iou
            }
            .validate(&b)
            .is_ok()
        );
        assert!(
            Metric::TargetMask {
                types: vec!["Alive".into()],
                mask: vec![true; 3],
                score: MaskScore::Iou
            }
            .validate(&b)
            .is_err()
        );
        assert!(
            Metric::Series {
                types: vec!["Alive".into()],
                target: vec![]
            }
            .validate(&b)
            .is_err()
        );
        assert!(Metric::Period { window: 1 }.validate(&b).is_err());
        assert!(Metric::Period { window: 2 }.validate(&b).is_ok());
        assert!(
            Objective::maximise(Metric::DensityClassification {
                types: ["Alive".into(), "Inactive".into()]
            })
            .validate(&b)
            .is_ok()
        );
    }

    #[test]
    fn ranges_are_the_documented_defaults() {
        let b = blinker();
        assert_eq!(Metric::Activity.range(&b, 50), [0.0, 1.0]);
        assert_eq!(Metric::Lifetime.range(&b, 50), [0.0, 50.0]);
        assert_eq!(Metric::Growth { types: vec![] }.range(&b, 50), [-1.0, 1.0]);
        assert_eq!(Metric::Period { window: 64 }.range(&b, 50), [0.0, 32.0]);
        assert_eq!(
            Metric::Entropy.range(&b, 50),
            [0.0, 1.0],
            "two declared types: log2(2)"
        );
        assert_eq!(Metric::Fraction { types: vec![] }.range(&b, 50), [0.0, 1.0]);
        assert_eq!(
            Metric::BboxFraction { types: vec![] }.range(&b, 50),
            [0.0, 1.0]
        );
        assert_eq!(
            Metric::CentroidSpeed { types: vec![] }.range(&b, 50),
            [0.0, 1.0]
        );
        assert_eq!(
            Metric::TargetMask {
                types: vec![],
                mask: vec![],
                score: MaskScore::Iou
            }
            .range(&b, 5),
            [0.0, 1.0]
        );
        assert_eq!(
            Metric::Series {
                types: vec![],
                target: vec![]
            }
            .range(&b, 5),
            [0.0, 1.0]
        );
        assert_eq!(
            Metric::DensityClassification {
                types: ["a".into(), "b".into()]
            }
            .range(&b, 5),
            [0.0, 1.0]
        );
    }

    #[test]
    fn objectives_round_trip_through_json_in_the_documented_shape() {
        let json = r#"{"metric": "fraction", "types": ["Alive"], "when": {"step": 100}, "goal": {"target": 0.3}}"#;
        let obj: Objective = serde_json::from_str(json).unwrap();
        assert_eq!(
            obj.metric,
            Metric::Fraction {
                types: vec!["Alive".into()]
            }
        );
        assert_eq!(obj.when, When::Step(100));
        assert_eq!(obj.goal, Goal::Target(0.3));
        let back: Objective = serde_json::from_str(&serde_json::to_string(&obj).unwrap()).unwrap();
        assert_eq!(back, obj);
        let minimal: Objective = serde_json::from_str(r#"{"metric": "lifetime"}"#).unwrap();
        assert_eq!(minimal, Objective::maximise(Metric::Lifetime));
        let american: Objective =
            serde_json::from_str(r#"{"metric": "entropy", "goal": "maximize"}"#).unwrap();
        assert_eq!(american.goal, Goal::Maximise);
        let period: Objective = serde_json::from_str(r#"{"metric": "period"}"#).unwrap();
        assert_eq!(period.metric, Metric::Period { window: 64 });
        // A flattened metric cannot refuse extra fields; a wrong metric name is refused.
        assert!(
            serde_json::from_str::<Objective>(r#"{"metric": "fraction", "types": [], "bogus": 1}"#)
                .is_ok()
        );
        assert!(serde_json::from_str::<Objective>(r#"{"metric": "nonsense"}"#).is_err());
        assert!(
            serde_json::from_str::<Objective>(r#"{"metric": "fraction"}"#).is_err(),
            "fraction needs types"
        );
    }

    #[test]
    fn elongation_is_one_for_blobs_and_large_for_streaks() {
        let alive = CellType::from("Alive");
        // A 4x4 block: as wide as it is long.
        let block: Vec<(usize, usize)> = (2..6).flat_map(|y| (2..6).map(move |x| (x, y))).collect();
        let e = elongation(&life(12, 12, &block), &[alive]);
        assert!((e - 1.0).abs() < 1e-9, "{e}");
        // A horizontal line of 10 cells, and the same line diagonally.
        let row: Vec<(usize, usize)> = (1..11).map(|x| (x, 5)).collect();
        let e_row = elongation(&life(12, 12, &row), &[alive]);
        assert!(e_row > 5.0 && e_row <= MAX_ELONGATION, "{e_row}");
        let diag: Vec<(usize, usize)> = (1..11).map(|x| (x, x)).collect();
        let e_diag = elongation(&life(12, 12, &diag), &[alive]);
        assert!(e_diag > 5.0, "direction does not matter: {e_diag}");
        // A 2:1 rectangle is about 2.
        let rect: Vec<(usize, usize)> = (2..10).flat_map(|x| (4..8).map(move |y| (x, y))).collect();
        let e_rect = elongation(&life(12, 12, &rect), &[alive]);
        assert!((e_rect - 2.0).abs() < 0.15, "{e_rect}");
        // Fewer than two cells: 1.
        assert_eq!(elongation(&life(12, 12, &[(3, 3)]), &[alive]), 1.0);
        assert_eq!(elongation(&life(12, 12, &[]), &[alive]), 1.0);
        // As a metric: descriptor, range [1, 10], sampled.
        let m = Metric::Elongation {
            types: vec!["Alive".into()],
        };
        assert!(m.is_descriptor());
        assert_eq!(m.range(&life(12, 12, &row), 10), [1.0, MAX_ELONGATION]);
        assert_eq!(m.sample(&life(12, 12, &row)), e_row);
        assert_eq!(m.type_names(), vec!["Alive"]);
    }
}
