//! Simple JSON configuration format to build grids without writing Rust code.
//! This format focuses on readability: you specify dimensions, history limit,
//! an initial array of type names, and the rule definition.
//!
//! A config file doubles as a save file: an optional `snapshot` block holds
//! the run-time state a config alone can't express (the current step, cells,
//! ages and history), so the same file that describes a scenario can also
//! resume one mid-run. `initial` always stays the scenario's starting
//! cells — the target [`crate::grid1d::Grid1D::reset_cells`]/
//! [`crate::grid2d::Grid2D::reset_cells`] would restore — never the
//! snapshot's. See [`CellaConfig::save_1d`]/[`CellaConfig::save_2d`] to
//! build one from a live grid, and [`CellaConfig::build_grid1d_resumed`]/
//! [`CellaConfig::build_grid2d_resumed`] to build the grid the snapshot
//! describes.
use crate::explore::{Ensemble, EnsembleConfig, Evolution, EvolveConfig, Sim};
use crate::external::ModelError;
use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;
use crate::rules::{Rule1D, Rule2D};
use crate::state::GridState;
use crate::types::{CellState, CellType};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

/// Top-level configuration for either a 1D or 2D automaton.
///
/// You can serialize/deserialize this enum to exchange scenarios.
///
/// Example (build from JSON string)
/// ```rust
/// use cella_lib::config::{CellaConfig, Config2D};
/// use cella_lib::{Rule2D, Rule2DSubrule, Neighborhood2D};
/// let json = serde_json::json!({
///   "dim":"2d",
///   "width":3,
///   "height":3,
///   "history_limit":2,
///   "initial":["Inactive","Inactive","Inactive","Inactive","Alive","Inactive","Inactive","Inactive","Inactive"],
///   "rule":{
///     "subrules":[
///       {"current_type":"Alive","criteria_type":"Alive","count":4,"op":"gt","range":1,"neighborhood":"Moore","randomness":null,"output_type":"Inactive"},
///       {"current_type":"Alive","criteria_type":"Alive","count":2,"op":"gt","range":1,"neighborhood":"Moore","randomness":null,"output_type":"Alive"},
///       {"current_type":"Inactive","criteria_type":"Alive","count":3,"op":"eq","range":1,"neighborhood":"Moore","randomness":null,"output_type":"Alive"}
///     ]
///   }
/// });
/// let cfg: CellaConfig = serde_json::from_value(json).unwrap();
/// assert!(cfg.build_grid2d().is_some());
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "dim")]
pub enum CellaConfig {
    #[serde(rename = "1d")]
    D1(Config1D),
    #[serde(rename = "2d")]
    D2(Config2D),
}

impl CellaConfig {
    /// The `colors` map of whichever variant this is (empty when the file had none).
    pub fn colors(&self) -> &BTreeMap<String, String> {
        match self {
            CellaConfig::D1(c) => &c.colors,
            CellaConfig::D2(c) => &c.colors,
        }
    }

    /// The `snapshot` block, if the file was saved mid-run (step > 0).
    pub fn snapshot(&self) -> Option<&RunSnapshot> {
        match self {
            CellaConfig::D1(c) => c.snapshot.as_ref(),
            CellaConfig::D2(c) => c.snapshot.as_ref(),
        }
    }
}

/// Run-time state a [`CellaConfig`] can't otherwise express: the step a run
/// had reached, plus its live cells, ages and per-cell history. Written only
/// when a save happens after step 0 — at step 0 the grid is its own
/// `initial`, so there is nothing here that `initial` doesn't already say.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RunSnapshot {
    /// Step the grid had reached when saved.
    pub step: u64,
    /// Current type name per cell, length = the config's cell count.
    pub cells: Vec<String>,
    /// Steps each cell has held its current type; same length as `cells`.
    pub ages: Vec<u32>,
    /// Per-cell history, oldest type first, capped at `history_limit`; same
    /// length as `cells`.
    pub history: Vec<Vec<String>>,
    /// Peak (max-so-far) count per type name since the run started. Kept
    /// explicitly because, unlike the current counts, it can't be recomputed
    /// from `cells` alone.
    pub peak_counts: BTreeMap<String, u64>,
}

/// 1D configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config1D {
    /// Grid width.
    pub width: usize,
    /// Per-cell history cap.
    pub history_limit: usize,
    /// Initial cell types by name, length = width.
    pub initial: Vec<String>,
    /// Rule definition.
    pub rule: Rule1D,
    /// Seed for the rule's `randomness` draws (default 0). Same seed, same
    /// run, on any thread count. See [`crate::Grid1D::seed`].
    #[serde(default)]
    pub seed: u64,
    /// Display colours by cell-type name, as `#rrggbb` hex strings, e.g.
    /// `{"Forest": "#2e8b57"}`. Optional; the engine never reads them — the
    /// GUI applies them on load, and any type not listed gets an automatic
    /// colour. `"Inactive"` sets the background colour.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
    /// Optional ensemble settings; see [`crate::explore::ensemble`] and
    /// `docs/explore.md`. [`CellaConfig::build_ensemble`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ensemble: Option<EnsembleConfig>,
    /// Optional evolution settings; see [`crate::explore::evolve`] and
    /// `docs/explore.md`. [`CellaConfig::build_evolution`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolve: Option<EvolveConfig>,
    /// A run in progress, saved mid-simulation; absent when saved at step 0.
    /// See the module docs and [`CellaConfig::build_grid1d_resumed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<RunSnapshot>,
}

impl Default for Config1D {
    fn default() -> Self {
        Config1D {
            width: 0,
            history_limit: 0,
            initial: Vec::new(),
            rule: Rule1D {
                subrules: Vec::new(),
            },
            seed: 0,
            colors: BTreeMap::new(),
            ensemble: None,
            evolve: None,
            snapshot: None,
        }
    }
}

/// 2D configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config2D {
    /// Grid width.
    pub width: usize,
    /// Grid height.
    pub height: usize,
    /// Per-cell history cap.
    pub history_limit: usize,
    /// Initial cell types by name, length = width*height.
    pub initial: Vec<String>,
    /// Rule definition.
    pub rule: Rule2D,
    /// Optional external transition model (e.g. `{"wildfire": {...}}`); when
    /// present it replaces the subrule engine. See [`crate::external`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Box<dyn crate::external::ExternalModel>>,
    /// Seed for the rule's `randomness` draws (default 0). Same seed, same
    /// run, on any thread count. A model keeps its own seed; ensembles and
    /// evolutions reseed both per member. See [`crate::Grid2D::seed`].
    #[serde(default)]
    pub seed: u64,
    /// Display colours by cell-type name, as `#rrggbb` hex strings, e.g.
    /// `{"Forest": "#2e8b57"}`. Optional; the engine never reads them — the
    /// GUI applies them on load, and any type not listed gets an automatic
    /// colour. `"Inactive"` sets the background colour.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
    /// Optional ensemble settings (members, genes, tracked types, learning
    /// operators, driver); works with any rule or model. See
    /// [`crate::explore::ensemble`] and `docs/explore.md`.
    /// [`CellaConfig::build_ensemble`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ensemble: Option<EnsembleConfig>,
    /// Optional evolution settings (population, genes, objective, search
    /// mode). See [`crate::explore::evolve`] and `docs/explore.md`.
    /// [`CellaConfig::build_evolution`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolve: Option<EvolveConfig>,
    /// A run in progress, saved mid-simulation; absent when saved at step 0.
    /// See the module docs and [`CellaConfig::build_grid2d_resumed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<RunSnapshot>,
}

impl Default for Config2D {
    fn default() -> Self {
        Config2D {
            width: 0,
            height: 0,
            history_limit: 0,
            initial: Vec::new(),
            rule: Rule2D {
                subrules: Vec::new(),
            },
            model: None,
            seed: 0,
            colors: BTreeMap::new(),
            ensemble: None,
            evolve: None,
            snapshot: None,
        }
    }
}

impl CellaConfig {
    /// Load configuration from a JSON file path.
    ///
    /// ```no_run
    /// use cella_lib::config::CellaConfig;
    /// let cfg = CellaConfig::from_file("configs/life.json").unwrap();
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let data = fs::read_to_string(path)?;
        let cfg: CellaConfig = serde_json::from_str(&data)?;
        Ok(cfg)
    }

    /// Save configuration to a JSON file path (pretty printed).
    ///
    /// ```no_run
    /// use cella_lib::config::{CellaConfig, Config2D};
    /// // write some cfg
    /// # let cfg: CellaConfig = serde_json::from_str("{\"dim\":\"2d\",\"width\":1,\"height\":1,\"history_limit\":1,\"initial\":[\"Inactive\"],\"rule\":{\"subrules\":[]}}").unwrap();
    /// cfg.to_file_pretty("out.json").unwrap();
    /// ```
    pub fn to_file_pretty<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let s = serde_json::to_string_pretty(self)?;
        fs::write(path, s)?;
        Ok(())
    }

    /// Build a Grid1D from D1 config.
    ///
    /// Returns `None` if `initial.len() != width`, or `history_limit > 255`.
    pub fn build_grid1d(&self) -> Option<Grid1D> {
        match self {
            CellaConfig::D1(c) => {
                // `Grid1D::new` asserts `history_limit <= 255` (the SoA
                // head/count arrays are `u8`); refuse a bad file instead of
                // panicking on it.
                if c.history_limit > 255 || c.initial.len() != c.width {
                    return None;
                }
                let init: Vec<CellType> = c.initial.iter().map(|s| CellType::new(s)).collect();
                Some(Grid1D::new(c.width, c.history_limit, init, c.rule.clone()).with_seed(c.seed))
            }
            _ => None,
        }
    }

    /// Build whichever grid this config describes, as a [`Sim`]. `None` on a
    /// length mismatch or a model that fails to attach.
    pub fn build_sim(&self) -> Option<Sim> {
        match self {
            CellaConfig::D1(_) => self.build_grid1d().map(Sim::D1),
            CellaConfig::D2(_) => self.build_grid2d().map(Sim::D2),
        }
    }

    /// The `ensemble` block, if the config has one.
    pub fn ensemble(&self) -> Option<&EnsembleConfig> {
        match self {
            CellaConfig::D1(c) => c.ensemble.as_ref(),
            CellaConfig::D2(c) => c.ensemble.as_ref(),
        }
    }

    /// The `evolve` block, if the config has one.
    pub fn evolve(&self) -> Option<&EvolveConfig> {
        match self {
            CellaConfig::D1(c) => c.evolve.as_ref(),
            CellaConfig::D2(c) => c.evolve.as_ref(),
        }
    }

    /// Set (or clear) the `ensemble` and `evolve` blocks, e.g. to carry a GUI
    /// session's Explore settings into a save file.
    pub fn set_explore_blocks(
        &mut self,
        ensemble: Option<EnsembleConfig>,
        evolve: Option<EvolveConfig>,
    ) {
        match self {
            CellaConfig::D1(c) => {
                c.ensemble = ensemble;
                c.evolve = evolve;
            }
            CellaConfig::D2(c) => {
                c.ensemble = ensemble;
                c.evolve = evolve;
            }
        }
    }

    /// Build the ensemble this config describes: the grid is built (and its
    /// model attached) once, then cloned per member with genes drawn from the
    /// `ensemble.genes` ranges. `None` when the config has no `ensemble`
    /// block or the grid cannot be built; `Some(Err)` when the block is
    /// invalid (a gene names an unknown knob, a tracked type is not declared,
    /// a free gene has no driver, ...).
    pub fn build_ensemble(&self) -> Option<Result<Ensemble, ModelError>> {
        let cfg = self.ensemble()?;
        let sim = self.build_sim()?;
        Some(Ensemble::new(sim, cfg))
    }

    /// Build the evolution this config describes from its `evolve` block.
    /// `None` when there is no block or the grid cannot be built;
    /// `Some(Err)` when the block is invalid.
    pub fn build_evolution(&self) -> Option<Result<Evolution, ModelError>> {
        let cfg = self.evolve()?;
        let sim = self.build_sim()?;
        Some(Evolution::new(sim, cfg))
    }

    /// Build a Grid2D from D2 config.
    ///
    /// Returns `None` if `width * height` overflows `usize`,
    /// `initial.len() != width*height`, `history_limit > 255` (`Grid2D::new`
    /// asserts this — the SoA head/count arrays are `u8`), or the config's
    /// external model fails validation against the grid.
    pub fn build_grid2d(&self) -> Option<Grid2D> {
        match self {
            CellaConfig::D2(c) => {
                let cell_count = c.width.checked_mul(c.height)?;
                if c.history_limit > 255 || c.initial.len() != cell_count {
                    return None;
                }
                let init: Vec<CellType> = c.initial.iter().map(|s| CellType::new(s)).collect();
                let mut grid =
                    Grid2D::new(c.width, c.height, c.history_limit, init, c.rule.clone())
                        .with_seed(c.seed);
                if let Some(model) = &c.model {
                    grid.attach_model(model.clone()).ok()?;
                }
                Some(grid)
            }
            _ => None,
        }
    }

    /// Build the 1D grid a saved `snapshot` describes, mid-run.
    ///
    /// Converts the config and snapshot into an in-memory
    /// [`GridState`] and hands it to [`Grid1D::from_state`], the same
    /// restore path a `Sim` uses — so the SoA history rebuild and count
    /// recompute happen in one place, not twice.
    ///
    /// Returns `None` when there is no `snapshot`, `history_limit > 255`,
    /// its `cells`, `ages` or `history` length doesn't match `width`, or any
    /// per-cell `history` entry is longer than `history_limit` (a
    /// hand-edited or truncated file, in every case).
    pub fn build_grid1d_resumed(&self) -> Option<Grid1D> {
        let CellaConfig::D1(c) = self else {
            return None;
        };
        // `Grid1D::from_state` builds the flat `u8` head/count arrays
        // directly (it doesn't go through `Grid1D::new`'s assert), so a
        // `history_limit` over 255 would silently truncate there instead of
        // panicking; refuse it up front, the same as `build_grid1d` does.
        if c.history_limit > 255 {
            return None;
        }
        let snap = c.snapshot.as_ref()?;
        if snap.cells.len() != c.width || snap.ages.len() != c.width || snap.history.len() != c.width
        {
            return None;
        }
        // A per-cell history longer than `history_limit` would overrun that
        // cell's slot in `state.rs`'s flat `soa_history` buffer, corrupting
        // (or, at the last cell, panicking past the end of) the next cell's
        // history. Reject it the same way a length mismatch is rejected.
        if snap.history.iter().any(|h| h.len() > c.history_limit) {
            return None;
        }
        let state = GridState::D1 {
            width: c.width,
            history_limit: c.history_limit,
            cell_states: snapshot_cell_states(c.history_limit, snap),
            step: snap.step,
            seed: c.seed,
            rule: c.rule.clone(),
            counts_current: HashMap::new(),
            peak_counts: snap.peak_counts.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        };
        Grid1D::from_state(&state)
    }

    /// Build the 2D grid a saved `snapshot` describes, mid-run. The model (if
    /// any) is re-attached fresh, the same as [`CellaConfig::build_grid2d`];
    /// see the module docs for what that means for a model with derived
    /// state it doesn't serialize (wildfire's `arrival` table, for one).
    ///
    /// Returns `None` when `width * height` overflows `usize`, there is no
    /// `snapshot`, `history_limit > 255`, its `cells`, `ages` or `history`
    /// length doesn't match `width * height`, any per-cell `history` entry
    /// is longer than `history_limit`, or the model fails to re-attach.
    pub fn build_grid2d_resumed(&self) -> Option<Grid2D> {
        let CellaConfig::D2(c) = self else {
            return None;
        };
        // See `build_grid1d_resumed`: `Grid2D::from_state` builds the flat
        // `u8` head/count arrays directly, so this would silently truncate
        // rather than panic if left unchecked.
        if c.history_limit > 255 {
            return None;
        }
        let snap = c.snapshot.as_ref()?;
        let n = c.width.checked_mul(c.height)?;
        if snap.cells.len() != n || snap.ages.len() != n || snap.history.len() != n {
            return None;
        }
        // See the matching check in `build_grid1d_resumed`: an oversized
        // per-cell history would overrun its slot in the flat SoA buffer.
        if snap.history.iter().any(|h| h.len() > c.history_limit) {
            return None;
        }
        let state = GridState::D2 {
            width: c.width,
            height: c.height,
            history_limit: c.history_limit,
            cell_states: snapshot_cell_states(c.history_limit, snap),
            step: snap.step,
            seed: c.seed,
            rule: c.rule.clone(),
            counts_current: HashMap::new(),
            peak_counts: snap.peak_counts.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            model: c.model.clone(),
        };
        Grid2D::from_state(&state)
    }

    /// Build a save file from a live 1D grid: the current rule, seed and
    /// colours, plus `initial`'s cells as the Reset target.
    ///
    /// At `current.step == 0` the grid shown *is* the start (including any
    /// cells painted before the first step), so `initial` is skipped in
    /// favour of the grid's own cells and no `snapshot` is written.
    /// Otherwise `initial` supplies the Reset target and the grid's run goes
    /// into `snapshot`. `ensemble`/`evolve` start as `None`; set them with
    /// [`CellaConfig::set_explore_blocks`].
    pub fn save_1d(
        initial: &GridState,
        current: &Grid1D,
        colors: BTreeMap<String, String>,
    ) -> CellaConfig {
        let (initial_cells, snapshot) = if current.step == 0 {
            (cell_names(current.cells()), None)
        } else {
            (
                state_cell_names(initial),
                Some(snapshot_from_state(&GridState::from_grid1d(current))),
            )
        };
        CellaConfig::D1(Config1D {
            width: current.width,
            history_limit: current.history_limit,
            initial: initial_cells,
            rule: current.rule.clone(),
            seed: current.seed,
            colors,
            ensemble: None,
            evolve: None,
            snapshot,
        })
    }

    /// Build a save file from a live 2D grid. See
    /// [`CellaConfig::save_1d`]; the model (if any) is carried over as-is.
    pub fn save_2d(
        initial: &GridState,
        current: &Grid2D,
        colors: BTreeMap<String, String>,
    ) -> CellaConfig {
        let (initial_cells, snapshot) = if current.step == 0 {
            (cell_names(current.cells()), None)
        } else {
            (
                state_cell_names(initial),
                Some(snapshot_from_state(&GridState::from_grid2d(current))),
            )
        };
        CellaConfig::D2(Config2D {
            width: current.width,
            height: current.height,
            history_limit: current.history_limit,
            initial: initial_cells,
            rule: current.rule.clone(),
            model: current.model.clone(),
            seed: current.seed,
            colors,
            ensemble: None,
            evolve: None,
            snapshot,
        })
    }
}

/// Type names for a slice of live cells, in order.
fn cell_names(cells: &[CellType]) -> Vec<String> {
    cells.iter().map(|c| c.as_str().to_string()).collect()
}

/// Type names for a [`GridState`]'s cells, in order — used for the `initial`
/// array so it survives a mid-run save (the live grid has moved on).
fn state_cell_names(state: &GridState) -> Vec<String> {
    match state {
        GridState::D1 { cell_states, .. } | GridState::D2 { cell_states, .. } => {
            cell_states.iter().map(|cs| cs.current.as_str().to_string()).collect()
        }
    }
}

/// A [`RunSnapshot`] describing whichever grid this [`GridState`] came from.
fn snapshot_from_state(state: &GridState) -> RunSnapshot {
    match state {
        GridState::D1 {
            cell_states,
            step,
            peak_counts,
            ..
        }
        | GridState::D2 {
            cell_states,
            step,
            peak_counts,
            ..
        } => RunSnapshot {
            step: *step,
            cells: cell_states.iter().map(|cs| cs.current.as_str().to_string()).collect(),
            ages: cell_states.iter().map(|cs| cs.age_in_state).collect(),
            history: cell_states
                .iter()
                .map(|cs| cs.history.iter().map(|t| t.as_str().to_string()).collect())
                .collect(),
            peak_counts: peak_counts.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        },
    }
}

/// Turn a snapshot's per-cell string data into the [`CellState`]s
/// [`GridState`] expects. Only called after a length check against the
/// config's cell count, so `snap.ages`/`snap.history` are the same length as
/// `snap.cells`.
fn snapshot_cell_states(history_limit: usize, snap: &RunSnapshot) -> Vec<CellState> {
    snap.cells
        .iter()
        .zip(&snap.ages)
        .zip(&snap.history)
        .map(|((ty, age), hist)| CellState {
            current: CellType::new(ty),
            age_in_state: *age,
            history_limit,
            history: hist.iter().map(|s| CellType::new(s)).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{CountOp, Neighborhood2D, Rule1DSubrule, Rule2DSubrule};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Drop zero-valued entries from a `Spur`-keyed count map. The engine's
    /// dominant-type bookkeeping (the majority type is skipped during
    /// per-cell counting and back-filled by subtraction) can leave a stale
    /// zero-valued entry in `counts_current`/`peak_counts` for a type that
    /// briefly held the majority — a live-stepped grid and one rebuilt
    /// through `from_state` (as every `_resumed` builder does) can disagree
    /// on exactly when that happens, even though neither has any actual
    /// cells of that type. A type simply absent from a map means the same
    /// thing as one present with count 0, so tests compare maps with those
    /// dropped rather than asserting exact `HashMap` equality.
    fn non_zero(m: &HashMap<lasso2::Spur, u64>) -> HashMap<lasso2::Spur, u64> {
        m.iter().filter(|&(_, &v)| v != 0).map(|(&k, &v)| (k, v)).collect()
    }

    #[test]
    fn build_grid_rejects_mismatched_initial_lengths() {
        let rule1 = Rule1D { subrules: vec![] };
        let cfg1 = CellaConfig::D1(Config1D {
            colors: Default::default(),
            seed: 0,
            ensemble: None,
            evolve: None,
            width: 3,
            history_limit: 1,
            initial: vec!["A".to_string(), "B".to_string()],
            rule: rule1,
            snapshot: None,
        });
        assert!(cfg1.build_grid1d().is_none());

        let rule2 = Rule2D { subrules: vec![] };
        let cfg2 = CellaConfig::D2(Config2D {
            colors: Default::default(),
            width: 2,
            height: 2,
            history_limit: 1,
            initial: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            rule: rule2,
            model: None,
            ensemble: None,
            evolve: None,
            seed: 0,
            snapshot: None,
        });
        assert!(cfg2.build_grid2d().is_none());
    }

    #[test]
    fn file_roundtrip_and_build_grid_success_paths() {
        let a = "A".to_string();
        let b = "B".to_string();

        let cfg1 = CellaConfig::D1(Config1D {
            colors: Default::default(),
            seed: 0,
            ensemble: None,
            evolve: None,
            width: 2,
            history_limit: 1,
            initial: vec![a.clone(), b.clone()],
            rule: Rule1D { subrules: vec![] },
            snapshot: None,
        });
        assert!(cfg1.build_grid1d().is_some());
        assert!(cfg1.build_grid2d().is_none());

        let cfg2 = CellaConfig::D2(Config2D {
            colors: Default::default(),
            width: 1,
            height: 2,
            history_limit: 1,
            initial: vec![a, b],
            rule: Rule2D { subrules: vec![] },
            model: None,
            ensemble: None,
            evolve: None,
            seed: 0,
            snapshot: None,
        });
        assert!(cfg2.build_grid2d().is_some());
        assert!(cfg2.build_grid1d().is_none());

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cella_cfg_{stamp}.json"));
        cfg2.to_file_pretty(&path).expect("write config");
        let loaded = CellaConfig::from_file(&path).expect("read config");
        assert!(matches!(loaded, CellaConfig::D2(_)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn file_io_error_paths_are_covered() {
        let missing = std::env::temp_dir().join("cella_missing_config_hopefully.json");
        assert!(CellaConfig::from_file(&missing).is_err());

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let bad_path = std::env::temp_dir().join(format!("cella_bad_cfg_{stamp}.json"));
        std::fs::write(&bad_path, "{not json").unwrap();
        assert!(CellaConfig::from_file(&bad_path).is_err());
        let _ = std::fs::remove_file(&bad_path);

        let cfg = CellaConfig::D1(Config1D {
            colors: Default::default(),
            seed: 0,
            ensemble: None,
            evolve: None,
            width: 1,
            history_limit: 0,
            initial: vec!["Inactive".to_string()],
            rule: Rule1D { subrules: vec![] },
            snapshot: None,
        });
        // Writing to a directory path fails, covering fs::write error propagation.
        let dir_path = std::env::temp_dir();
        assert!(cfg.to_file_pretty(dir_path).is_err());
    }

    #[test]
    fn save_at_step_0_has_no_snapshot_and_uses_the_grid_as_initial() {
        let x = CellType::from("X");
        let g = Grid1D::new(3, 2, vec![x, CellType::inactive(), x], Rule1D { subrules: vec![] });
        let initial = GridState::from_grid1d(&g);
        let cfg = CellaConfig::save_1d(&initial, &g, BTreeMap::new());
        assert!(cfg.snapshot().is_none());
        let CellaConfig::D1(c) = &cfg else {
            panic!("expected D1");
        };
        assert_eq!(c.initial, vec!["X", "Inactive", "X"]);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("snapshot"), "no snapshot key at step 0");

        let g2d = Grid2D::new(
            2,
            1,
            2,
            vec![x, CellType::inactive()],
            Rule2D { subrules: vec![] },
        );
        let initial2d = GridState::from_grid2d(&g2d);
        let cfg2d = CellaConfig::save_2d(&initial2d, &g2d, BTreeMap::new());
        assert!(cfg2d.snapshot().is_none());
        let json2d = serde_json::to_string(&cfg2d).unwrap();
        assert!(!json2d.contains("snapshot"));
    }

    #[test]
    fn save_mid_run_then_build_resumed_gives_the_same_run_1d() {
        let x = CellType::from("X");
        let rule = Rule1D {
            subrules: vec![Rule1DSubrule {
                current_type: x,
                criteria_type: x,
                wolfram_code: 0xFF,
                n: 1,
                randomness: None,
                output_type: x,
            }],
        };
        let mut g = Grid1D::new(
            5,
            3,
            vec![
                x,
                CellType::inactive(),
                x,
                CellType::inactive(),
                CellType::inactive(),
            ],
            rule,
        );
        let initial = GridState::from_grid1d(&g);
        for _ in 0..4 {
            g.step();
        }
        let cfg = CellaConfig::save_1d(&initial, &g, BTreeMap::new());
        let snap = cfg.snapshot().expect("saved mid-run");
        assert_eq!(snap.step, g.step);

        // Reset target stays the run's start, not where it was saved.
        let CellaConfig::D1(c) = &cfg else {
            panic!("expected D1");
        };
        assert_eq!(c.initial, vec!["X", "Inactive", "X", "Inactive", "Inactive"]);

        // No duplicated keys: the per-cell `history_limit` copy stays gone,
        // and nothing under `snapshot` repeats a top-level config key.
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("history_limit\":3,\"current\""));
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let snapshot_obj = value["snapshot"].as_object().expect("snapshot object");
        assert!(!snapshot_obj.contains_key("width"));
        assert!(!snapshot_obj.contains_key("history_limit"));

        let restored = cfg.build_grid1d_resumed().expect("resumed build");
        assert_eq!(restored.step, g.step);
        for i in 0..g.width {
            assert_eq!(restored.cell_type(i), g.cell_type(i), "cell {i}");
            assert_eq!(restored.cell_age(i), g.cell_age(i), "age {i}");
            assert_eq!(restored.cell_history(i), g.cell_history(i), "history {i}");
        }
        assert_eq!(restored.peak_counts, g.peak_counts);

        // `build_grid1d` (not resumed) gives the initial state instead.
        let at_start = cfg.build_grid1d().expect("plain build");
        assert_eq!(at_start.step, 0);
        assert_eq!(at_start.cell_type(0), x);
        assert_eq!(at_start.cell_type(1), CellType::inactive());
    }

    #[test]
    fn save_mid_run_then_build_resumed_gives_the_same_run_2d() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                None,
                None,
            )],
        };
        let mut init = vec![a; 9];
        init[4] = b;
        let mut g = Grid2D::new(3, 3, 2, init, rule);
        let initial = GridState::from_grid2d(&g);
        for _ in 0..3 {
            g.step();
        }
        let colors: BTreeMap<String, String> =
            [("A".to_string(), "#112233".to_string())].into_iter().collect();
        let cfg = CellaConfig::save_2d(&initial, &g, colors.clone());
        assert_eq!(cfg.colors(), &colors);
        let snap = cfg.snapshot().expect("saved mid-run");
        assert_eq!(snap.step, g.step);
        assert_eq!(snap.peak_counts.len(), g.peak_counts.len());

        let restored = cfg.build_grid2d_resumed().expect("resumed build");
        assert_eq!(restored.step, g.step);
        for i in 0..(g.width * g.height) {
            assert_eq!(restored.cell_type(i), g.cell_type(i), "cell {i}");
            assert_eq!(restored.cell_age(i), g.cell_age(i), "age {i}");
        }
        assert_eq!(restored.counts_current, g.counts_current);
        assert_eq!(restored.peak_counts, g.peak_counts);
    }

    /// A non-zero seed, carried by subrules with `randomness` (so stepping
    /// actually depends on it), must survive `save_2d` → JSON → `build_grid2d_resumed`.
    ///
    /// The rule is an A↔B coin-flip oscillator: every step, an `A` cell
    /// becomes `B` with probability 0.5 (`cell_rand(seed, step, idx,
    /// STREAM_RULE) >= 0.5`, else a fallback subrule — no `randomness`, so it
    /// always matches — keeps it `A`), and a `B` cell flips back to `A` the
    /// same way. Every step, for every cell, exercises a fresh
    /// `cell_rand(seed, step, ..)` draw, so which cells are `A` vs `B`
    /// depends on `seed` at every step, not just the one the snapshot was
    /// taken at. If `save_2d` or `build_grid2d_resumed` hard-coded seed 0
    /// instead of threading the config's own seed through, `resumed.seed`
    /// below would read 0 instead of `SEED`, and — because `SEED != 0` draws
    /// different bits from `cell_rand` than seed 0 for the same `(step, idx,
    /// stream)` with overwhelming probability across 16 cells × 4 further
    /// steps — the post-resume cell types would almost certainly diverge
    /// from the uninterrupted reference too.
    #[test]
    fn a_non_zero_seed_and_stochastic_subrule_survive_save_and_resume_2d() {
        const SEED: u64 = 987_654_321;
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![
                // A -> B w.p. 0.5; else falls through to the next subrule.
                Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, Some(0.5), None),
                // Fallback: A stays A (no `randomness`, always matches).
                Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, a, None, None),
                // B -> A w.p. 0.5; else falls through to the next subrule.
                Rule2DSubrule::new(b, b, 0, CountOp::Gt, 1, Neighborhood2D::Moore, a, Some(0.5), None),
                // Fallback: B stays B.
                Rule2DSubrule::new(b, b, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, None, None),
            ],
        };
        let mut reference = Grid2D::new(4, 4, 3, vec![a; 16], rule.clone()).with_seed(SEED);
        let mut g = Grid2D::new(4, 4, 3, vec![a; 16], rule).with_seed(SEED);
        let initial = GridState::from_grid2d(&g);
        for _ in 0..4 {
            reference.step();
            g.step();
        }

        let cfg = CellaConfig::save_2d(&initial, &g, BTreeMap::new());
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: CellaConfig = serde_json::from_str(&json).unwrap();
        let mut resumed = cfg2.build_grid2d_resumed().expect("resumed build");
        assert_eq!(
            resumed.seed, SEED,
            "the config's own seed carried through, not hard-coded to 0"
        );

        for _ in 0..4 {
            reference.step();
            resumed.step();
        }
        for i in 0..16 {
            assert_eq!(reference.cell_type(i), resumed.cell_type(i), "cell {i}");
            assert_eq!(reference.cell_age(i), resumed.cell_age(i), "age {i}");
        }
        assert_eq!(
            non_zero(&reference.counts_current),
            non_zero(&resumed.counts_current)
        );
        assert_eq!(non_zero(&reference.peak_counts), non_zero(&resumed.peak_counts));
        // Sanity check that the scenario is actually exercising `randomness`:
        // an all-A grid where nothing ever matched would make this test
        // vacuous. Some, but not all, cells must have flipped to B.
        let b_count = (0..16).filter(|&i| reference.cell_type(i) == b).count();
        assert!(
            (1..16).contains(&b_count),
            "expected a mix of A and B from the stochastic subrule, got {b_count} B cells"
        );
    }

    /// 1D counterpart of the 2D seed test above (same A↔B coin-flip
    /// oscillator, so the same reasoning applies) — cheaper, so kept
    /// alongside it.
    #[test]
    fn a_non_zero_seed_and_stochastic_subrule_survive_save_and_resume_1d() {
        const SEED: u64 = 13_579;
        let a = CellType::from("A");
        let b = CellType::from("B");
        let any = 0xFFu128;
        let rule = Rule1D {
            subrules: vec![
                Rule1DSubrule {
                    current_type: a,
                    criteria_type: a,
                    wolfram_code: any,
                    n: 1,
                    randomness: Some(0.5),
                    output_type: b,
                },
                Rule1DSubrule {
                    current_type: a,
                    criteria_type: a,
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: a,
                },
                Rule1DSubrule {
                    current_type: b,
                    criteria_type: b,
                    wolfram_code: any,
                    n: 1,
                    randomness: Some(0.5),
                    output_type: a,
                },
                Rule1DSubrule {
                    current_type: b,
                    criteria_type: b,
                    wolfram_code: any,
                    n: 1,
                    randomness: None,
                    output_type: b,
                },
            ],
        };
        let mut reference = Grid1D::new(16, 3, vec![a; 16], rule.clone()).with_seed(SEED);
        let mut g = Grid1D::new(16, 3, vec![a; 16], rule).with_seed(SEED);
        let initial = GridState::from_grid1d(&g);
        for _ in 0..4 {
            reference.step();
            g.step();
        }

        let cfg = CellaConfig::save_1d(&initial, &g, BTreeMap::new());
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: CellaConfig = serde_json::from_str(&json).unwrap();
        let mut resumed = cfg2.build_grid1d_resumed().expect("resumed build");
        assert_eq!(
            resumed.seed, SEED,
            "the config's own seed carried through, not hard-coded to 0"
        );

        for _ in 0..4 {
            reference.step();
            resumed.step();
        }
        for i in 0..16 {
            assert_eq!(reference.cell_type(i), resumed.cell_type(i), "cell {i}");
            assert_eq!(reference.cell_age(i), resumed.cell_age(i), "age {i}");
        }
        assert_eq!(
            non_zero(&reference.counts_current),
            non_zero(&resumed.counts_current)
        );
        assert_eq!(non_zero(&reference.peak_counts), non_zero(&resumed.peak_counts));
        let b_count = (0..16).filter(|&i| reference.cell_type(i) == b).count();
        assert!(
            (1..16).contains(&b_count),
            "expected a mix of A and B from the stochastic subrule, got {b_count} B cells"
        );
    }

    #[test]
    fn build_resumed_rejects_a_missing_or_mismatched_snapshot() {
        let rule1 = Rule1D { subrules: vec![] };
        let cfg_no_snapshot = CellaConfig::D1(Config1D {
            width: 2,
            history_limit: 1,
            initial: vec!["A".to_string(), "B".to_string()],
            rule: rule1.clone(),
            ..Config1D::default()
        });
        assert!(cfg_no_snapshot.build_grid1d_resumed().is_none());
        assert!(CellaConfig::D2(Config2D::default())
            .build_grid2d_resumed()
            .is_none());

        let cfg_bad_lengths = CellaConfig::D1(Config1D {
            width: 2,
            history_limit: 1,
            initial: vec!["A".to_string(), "B".to_string()],
            rule: rule1,
            snapshot: Some(RunSnapshot {
                step: 3,
                cells: vec!["A".to_string()], // wrong length
                ages: vec![0],
                history: vec![vec![]],
                peak_counts: BTreeMap::new(),
            }),
            ..Config1D::default()
        });
        assert!(cfg_bad_lengths.build_grid1d_resumed().is_none());

        let rule2 = Rule2D { subrules: vec![] };
        let cfg2_bad_lengths = CellaConfig::D2(Config2D {
            width: 2,
            height: 2,
            history_limit: 1,
            initial: vec!["A".to_string(); 4],
            rule: rule2,
            snapshot: Some(RunSnapshot {
                step: 1,
                cells: vec!["A".to_string(); 3], // wrong length (need 4)
                ages: vec![0; 3],
                history: vec![vec![]; 3],
                peak_counts: BTreeMap::new(),
            }),
            ..Config2D::default()
        });
        assert!(cfg2_bad_lengths.build_grid2d_resumed().is_none());
    }

    #[test]
    fn build_resumed_refuses_the_wrong_dimension_up_front() {
        // Calling the 1D resume path on a saved 2D config (or vice versa)
        // must not panic or reach for the wrong variant's fields -- both
        // return `None` before looking at `snapshot` at all.
        assert!(CellaConfig::D2(Config2D::default())
            .build_grid1d_resumed()
            .is_none());
        assert!(CellaConfig::D1(Config1D::default())
            .build_grid2d_resumed()
            .is_none());
    }

    #[test]
    fn build_resumed_rejects_a_per_cell_history_longer_than_history_limit() {
        // A per-cell history longer than `history_limit` would overrun that
        // cell's slot in `state::soa_history`'s flat buffer (see the doc
        // comments on `build_grid1d_resumed`/`build_grid2d_resumed`), so this
        // must be rejected the same as a length mismatch, not panic or
        // corrupt a neighbouring cell's history.
        let rule1 = Rule1D { subrules: vec![] };
        let cfg1 = CellaConfig::D1(Config1D {
            width: 2,
            history_limit: 1,
            initial: vec!["A".to_string(), "B".to_string()],
            rule: rule1,
            snapshot: Some(RunSnapshot {
                step: 3,
                cells: vec!["A".to_string(), "B".to_string()],
                ages: vec![0, 0],
                // Right length (2 cells), but cell 0's history has 2 entries
                // where history_limit only allows 1.
                history: vec![vec!["A".to_string(), "B".to_string()], vec![]],
                peak_counts: BTreeMap::new(),
            }),
            ..Config1D::default()
        });
        assert!(
            cfg1.build_grid1d_resumed().is_none(),
            "an oversized per-cell history must be rejected, not overrun the SoA buffer"
        );

        let rule2 = Rule2D { subrules: vec![] };
        let cfg2 = CellaConfig::D2(Config2D {
            width: 2,
            height: 1,
            history_limit: 1,
            initial: vec!["A".to_string(); 2],
            rule: rule2,
            snapshot: Some(RunSnapshot {
                step: 3,
                cells: vec!["A".to_string(); 2],
                ages: vec![0; 2],
                history: vec![vec!["A".to_string(), "A".to_string()], vec![]],
                peak_counts: BTreeMap::new(),
            }),
            ..Config2D::default()
        });
        assert!(
            cfg2.build_grid2d_resumed().is_none(),
            "an oversized per-cell history must be rejected, not overrun the SoA buffer"
        );
    }

    #[test]
    fn build_rejects_a_history_limit_over_255() {
        // `Grid1D::new`/`Grid2D::new` assert `history_limit <= 255` (the SoA
        // head/count arrays are `u8`), and the `_resumed` builders skip
        // `new` entirely (they build the flat arrays directly), so both
        // families need their own guard rather than relying on the assert
        // — a hand-edited config with `history_limit: 300` must come back
        // `None`, not panic the caller.
        let rule1 = Rule1D { subrules: vec![] };
        let cfg1 = CellaConfig::D1(Config1D {
            width: 1,
            history_limit: 256,
            initial: vec!["A".to_string()],
            rule: rule1,
            snapshot: Some(RunSnapshot {
                step: 1,
                cells: vec!["A".to_string()],
                ages: vec![0],
                history: vec![vec![]],
                peak_counts: BTreeMap::new(),
            }),
            ..Config1D::default()
        });
        assert!(cfg1.build_grid1d().is_none());
        assert!(cfg1.build_grid1d_resumed().is_none());

        let rule2 = Rule2D { subrules: vec![] };
        let cfg2 = CellaConfig::D2(Config2D {
            width: 1,
            height: 1,
            history_limit: 256,
            initial: vec!["A".to_string()],
            rule: rule2,
            snapshot: Some(RunSnapshot {
                step: 1,
                cells: vec!["A".to_string()],
                ages: vec![0],
                history: vec![vec![]],
                peak_counts: BTreeMap::new(),
            }),
            ..Config2D::default()
        });
        assert!(cfg2.build_grid2d().is_none());
        assert!(cfg2.build_grid2d_resumed().is_none());
    }

    #[test]
    fn build_grid2d_rejects_a_width_height_overflow() {
        // `width * height` used to be a bare multiply; on a hand-edited
        // config with a huge `width`, that overflows `usize` (a debug-mode
        // panic, and silent wraparound in release) before the length checks
        // even run. `checked_mul` must catch it and return `None` instead.
        let rule = Rule2D { subrules: vec![] };
        let cfg = CellaConfig::D2(Config2D {
            width: usize::MAX,
            height: 2,
            history_limit: 1,
            initial: vec![],
            rule: rule.clone(),
            ..Config2D::default()
        });
        assert!(cfg.build_grid2d().is_none());

        let cfg_resumed = CellaConfig::D2(Config2D {
            width: usize::MAX,
            height: 2,
            history_limit: 1,
            initial: vec![],
            rule,
            snapshot: Some(RunSnapshot {
                step: 1,
                cells: vec![],
                ages: vec![],
                history: vec![],
                peak_counts: BTreeMap::new(),
            }),
            ..Config2D::default()
        });
        assert!(cfg_resumed.build_grid2d_resumed().is_none());
    }

    #[test]
    fn build_resumed_rejects_when_exactly_one_snapshot_field_is_the_wrong_length() {
        // `cells`, `ages` and `history` are three separate length checks;
        // each must independently reject a short file rather than the check
        // only firing when *all three* happen to agree with each other (but
        // not with the cell count).
        let rule1 = Rule1D { subrules: vec![] };
        let cfg1 = |cells: Vec<String>, ages: Vec<u32>, history: Vec<Vec<String>>| {
            CellaConfig::D1(Config1D {
                width: 2,
                history_limit: 2,
                initial: vec!["A".to_string(); 2],
                rule: rule1.clone(),
                snapshot: Some(RunSnapshot {
                    step: 1,
                    cells,
                    ages,
                    history,
                    peak_counts: BTreeMap::new(),
                }),
                ..Config1D::default()
            })
        };
        let ok_cells = vec!["A".to_string(), "B".to_string()];
        let ok_ages = vec![0u32, 1];
        let ok_history: Vec<Vec<String>> = vec![vec![], vec!["A".to_string()]];

        assert!(
            cfg1(vec!["A".to_string()], ok_ages.clone(), ok_history.clone())
                .build_grid1d_resumed()
                .is_none(),
            "cells too short"
        );
        assert!(
            cfg1(ok_cells.clone(), vec![0], ok_history.clone())
                .build_grid1d_resumed()
                .is_none(),
            "ages too short"
        );
        assert!(
            cfg1(ok_cells.clone(), ok_ages.clone(), vec![vec![]])
                .build_grid1d_resumed()
                .is_none(),
            "history too short"
        );
        assert!(
            cfg1(ok_cells, ok_ages, ok_history).build_grid1d_resumed().is_some(),
            "all three the right length must actually build"
        );

        let rule2 = Rule2D { subrules: vec![] };
        let cfg2 = |cells: Vec<String>, ages: Vec<u32>, history: Vec<Vec<String>>| {
            CellaConfig::D2(Config2D {
                width: 2,
                height: 2,
                history_limit: 2,
                initial: vec!["A".to_string(); 4],
                rule: rule2.clone(),
                snapshot: Some(RunSnapshot {
                    step: 1,
                    cells,
                    ages,
                    history,
                    peak_counts: BTreeMap::new(),
                }),
                ..Config2D::default()
            })
        };
        let ok_cells2 = vec!["A".to_string(); 4];
        let ok_ages2 = vec![0u32; 4];
        let ok_history2: Vec<Vec<String>> = vec![vec![]; 4];

        assert!(
            cfg2(vec!["A".to_string(); 3], ok_ages2.clone(), ok_history2.clone())
                .build_grid2d_resumed()
                .is_none(),
            "cells too short"
        );
        assert!(
            cfg2(ok_cells2.clone(), vec![0u32; 3], ok_history2.clone())
                .build_grid2d_resumed()
                .is_none(),
            "ages too short"
        );
        assert!(
            cfg2(ok_cells2.clone(), ok_ages2.clone(), vec![vec![]; 3])
                .build_grid2d_resumed()
                .is_none(),
            "history too short"
        );
        assert!(
            cfg2(ok_cells2, ok_ages2, ok_history2)
                .build_grid2d_resumed()
                .is_some(),
            "all three the right length must actually build"
        );
    }

    /// A 2D snapshot whose cells hold 0, 1 and 2 history entries (fewer than
    /// `history_limit` 3) — the normal early-run case, which must stay
    /// accepted. Since cells sharing one grid's step counter always gain a
    /// history entry every step in lockstep, there is no way to get three
    /// different pre-fill depths in one grid other than building each depth
    /// in its own little reference grid and combining them into one
    /// snapshot; those same three references, continued the ordinary way
    /// (never interrupted by save/resume), are what the resumed grid is
    /// checked against after stepping further. Matching them proves the
    /// ring buffer's head and count are right for a partially filled
    /// history, not just a full one.
    #[test]
    fn resuming_a_partially_filled_history_matches_an_uninterrupted_reference() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let c = CellType::from("C");
        // Neighbour-independent 3-cycle (A->B->C->A), so a lone 1-cell
        // reference grid behaves exactly like a cell inside the bigger
        // resumed grid: `Gt` with `count: 0` is "neighbour count >= 0",
        // trivially true regardless of how many neighbours actually exist.
        let rule = Rule2D {
            subrules: vec![
                Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, None, None),
                Rule2DSubrule::new(b, b, 0, CountOp::Gt, 1, Neighborhood2D::Moore, c, None, None),
                Rule2DSubrule::new(c, c, 0, CountOp::Gt, 1, Neighborhood2D::Moore, a, None, None),
            ],
        };
        let history_limit = 3;
        let mut refs: Vec<Grid2D> = (0u32..3)
            .map(|pre_steps| {
                let mut g = Grid2D::new(1, 1, history_limit, vec![a], rule.clone());
                for _ in 0..pre_steps {
                    g.step();
                }
                g
            })
            .collect();

        let cells: Vec<String> = refs.iter().map(|g| g.cell_type(0).as_str().to_string()).collect();
        let ages: Vec<u32> = refs.iter().map(|g| g.cell_age(0)).collect();
        let history: Vec<Vec<String>> = refs
            .iter()
            .map(|g| g.cell_history(0).iter().map(|t| t.as_str().to_string()).collect())
            .collect();
        assert_eq!(
            history.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "sanity check: the three references really do hold 0, 1 and 2 entries"
        );

        let cfg = CellaConfig::D2(Config2D {
            width: 3,
            height: 1,
            history_limit,
            initial: cells.clone(),
            rule,
            snapshot: Some(RunSnapshot {
                step: 2,
                cells,
                ages,
                history,
                peak_counts: BTreeMap::new(),
            }),
            ..Config2D::default()
        });
        let mut resumed = cfg
            .build_grid2d_resumed()
            .expect("a partially filled history still resumes");

        for _ in 0..4 {
            resumed.step();
            for g in refs.iter_mut() {
                g.step();
            }
        }

        for (i, r) in refs.iter().enumerate() {
            assert_eq!(resumed.cell_type(i), r.cell_type(0), "cell {i} type");
            assert_eq!(resumed.cell_age(i), r.cell_age(0), "cell {i} age");
            assert_eq!(resumed.cell_history(i), r.cell_history(0), "cell {i} history");
        }
    }

    #[test]
    fn set_explore_blocks_sets_and_clears_both_variants() {
        let mut cfg2 = CellaConfig::D2(Config2D {
            width: 1,
            height: 2,
            initial: vec!["A".to_string(), "B".to_string()],
            ..Config2D::default()
        });
        cfg2.set_explore_blocks(Some(EnsembleConfig::default()), None);
        assert!(cfg2.ensemble().is_some() && cfg2.evolve().is_none());

        let mut cfg1 = CellaConfig::D1(Config1D {
            width: 2,
            initial: vec!["A".to_string(), "B".to_string()],
            ..Config1D::default()
        });
        cfg1.set_explore_blocks(Some(EnsembleConfig::default()), None);
        assert!(cfg1.ensemble().is_some() && cfg1.evolve().is_none());
    }
}
