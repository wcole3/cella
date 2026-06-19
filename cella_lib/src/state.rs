//! Grid state snapshots and (de)serialization helpers.

use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;
use crate::rules::{Rule1D, Rule2D};
use crate::types::{interner, CellState};
use crate::CellType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use lasso2::Spur;

/// Serializable snapshot of either a 1D or 2D grid.
///
/// Use this to save and restore simulations across runs.
///
/// Example
/// ```rust
/// use cella_lib::{Grid2D, Rule2D, Rule2DSubrule, Neighborhood2D, CellType, GridState, CountOp};
/// let alive = CellType::from("Alive");
/// let inactive = CellType::inactive();
/// let rule = Rule2D { subrules: vec![
///   // Overpopulation: Alive with 4+ Alive neighbors becomes Inactive
///   Rule2DSubrule::new(alive.clone(), alive.clone(), 4, CountOp::Gt, 1, Neighborhood2D::Moore, inactive.clone(), None, None),
///   // Survival: Alive stays Alive if at least 2 Alive neighbors (after overpop check)
///   Rule2DSubrule::new(alive.clone(), alive.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, alive.clone(), None, None),
///   // Birth: Inactive becomes Alive if exactly 3 Alive neighbors
///   Rule2DSubrule::new(inactive.clone(), alive.clone(), 3, CountOp::Eq, 1, Neighborhood2D::Moore, alive.clone(), None, None),
/// ]};
/// let (w,h) = (4usize, 4usize);
/// let mut init = vec![CellType::inactive(); w*h];
/// init[1*w + 1] = alive.clone();
/// init[1*w + 2] = alive.clone();
/// init[1*w + 3.min(w-1)] = alive.clone();
/// let mut g = Grid2D::new(w, h, 3, init, rule);
/// g.step();
/// let st = GridState::from_grid2d(&g);
/// let json = st.to_json();
/// let st2 = GridState::from_json(&json).unwrap();
/// assert!(matches!(st2, GridState::D2{..}));
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GridState {
    D1 {
        width: usize,
        history_limit: usize,
        cell_states: Vec<CellState>,
        step: u64,
        rule: Rule1D,
        #[serde(default)] counts_current: HashMap<String, u64>,
        #[serde(default)] peak_counts: HashMap<String, u64>,
    },
    D2 {
        width: usize,
        height: usize,
        history_limit: usize,
        cell_states: Vec<CellState>,
        step: u64,
        rule: Rule2D,
        #[serde(default)] counts_current: HashMap<String, u64>,
        #[serde(default)] peak_counts: HashMap<String, u64>,
    },
}

impl GridState {
    /// Snapshot a 1D grid.
    pub fn from_grid1d(g: &Grid1D) -> Self {
        // convert the Spur maps into String keys
        let (current_count_map, peak_count_map)
            = convert_map_spur_to_string(&g.counts_current, &g.peak_counts);
        // build the GridState
        Self::D1 { width: g.width,
        history_limit: g.history_limit, cell_states: g.cell_states.clone(), step: g.step,
        rule: g.rule.clone(), counts_current: current_count_map,
        peak_counts: peak_count_map }
    }
    /// Snapshot a 2D grid.
    pub fn from_grid2d(g: &Grid2D) -> Self {
        let (current_count_map, peak_count_map)
            = convert_map_spur_to_string(&g.counts_current, &g.peak_counts);
        // build the GridState
        Self::D2 { width: g.width, height: g.height, history_limit: g.history_limit,
            cell_states: g.cell_states.clone(), step: g.step, rule: g.rule.clone(),
            counts_current: current_count_map, peak_counts: peak_count_map }
    }

    /// Serialize to pretty JSON.
    pub fn to_json_pretty(&self) -> String { serde_json::to_string_pretty(self).unwrap() }
    /// Serialize to compact JSON.
    pub fn to_json(&self) -> String { serde_json::to_string(self).unwrap() }

    /// Deserialize from JSON string.
    pub fn from_json(s: &str) -> serde_json::Result<Self> { serde_json::from_str(s) }
}

impl Grid1D {
    /// Build a Grid1D from a matching GridState variant.
    pub fn from_state(state: &GridState) -> Option<Self> {
        match state {
            GridState::D1 { width, history_limit, cell_states, step,
                rule, counts_current, peak_counts } => {
                let (new_counts, new_peak_counts) =
                    convert_map_string_to_spur(cell_states, counts_current, peak_counts);
                let cells: Vec<CellType> = cell_states.iter().map(|c| c.current).collect();
                let next_cells: Vec<CellType> = vec![CellType::inactive(); *width];
                Some(Self { width: *width, history_limit: *history_limit,
                    cell_states: cell_states.clone(), cells, next_cells,
                    step: *step, rule: rule.clone(), counts_current: new_counts, peak_counts: new_peak_counts,
                    inactive: CellType::inactive() })
            }
            _ => None,
        }
    }
}

impl Grid2D {
    /// Build a Grid2D from a matching GridState variant.
    pub fn from_state(state: &GridState) -> Option<Self> {
        match state {
            GridState::D2 { width, height, history_limit, cell_states,
                step, rule, counts_current, peak_counts } => {
                let (new_counts, new_peak_counts) =
                    convert_map_string_to_spur(cell_states, counts_current, peak_counts);
                // compute the cells and next_cells vecs
                let next_cells: Vec<CellType> = vec![CellType::inactive(); cell_states.len()];
                let cells: Vec<CellType> = cell_states.iter().map(|c| c.current).collect();
                Some(Self { width: *width, height: *height, history_limit: *history_limit,
                    cell_states: cell_states.clone(), cells, next_cells, step: *step, rule: rule.clone(), counts_current: new_counts,
                    peak_counts: new_peak_counts, inactive: CellType::inactive() })
            }
            _ => None,
        }
    }
}

/// Back-compat helper kept for examples.
///
/// Prefer `GridState::from_grid2d(&g).to_json_pretty()`.
pub fn grid2d_to_json(g: &Grid2D) -> String { GridState::from_grid2d(g).to_json_pretty() }


/// Helper for converting from HashMap<String, u64> to HashMap<Spur, u64>
fn convert_map_string_to_spur(cells: &Vec<CellState>, counts_current: &HashMap<String, u64>,
                                  peak_counts: &HashMap<String, u64>) -> (HashMap<Spur, u64>, HashMap<Spur, u64>) {
    let mut new_counts: HashMap<Spur, u64> = HashMap::new();
    let mut new_peak_counts: HashMap<Spur, u64> = HashMap::new();
    if counts_current.is_empty() {
        // populate with cell counts
        for c in cells { *new_counts.entry(c.current.0).or_insert(0) += 1; }
    } else {
        // copy from existing counts
        for (k, v) in counts_current.iter() {
            new_counts.insert(interner().get_or_intern(k), *v);
        }
    }
    // populate with peak counts
    if peak_counts.is_empty() {
        // start with current counts
        new_peak_counts = new_counts.clone();
    } else {
        for (k, v) in peak_counts.iter() {
            new_peak_counts.insert(interner().get_or_intern(k), *v);
        }
    }
    (new_counts, new_peak_counts)
}

// Helper for converting from HashMap<Spur, u64> to HashMap<String, u64>
fn convert_map_spur_to_string(counts_current: &HashMap<Spur, u64>, peak_counts: &HashMap<Spur, u64>)
    -> (HashMap<String, u64>, HashMap<String, u64>) {
    let mut new_counts: HashMap<String, u64> = HashMap::new();
    let mut new_peak_counts: HashMap<String, u64> = HashMap::new();
    for (k, v) in counts_current.iter() {
        new_counts.insert(interner().resolve(k).to_string(), *v);
    }
    for (k, v) in peak_counts.iter() {
        new_peak_counts.insert(interner().resolve(k).to_string(), *v);
    }
    (new_counts, new_peak_counts)
}

