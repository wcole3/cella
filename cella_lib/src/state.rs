//! Grid state snapshots and (de)serialization helpers.
use serde::{Deserialize, Serialize};
use crate::types::{CellState};
use crate::rules::{Rule1D, Rule2D};
use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;

/// Serializable snapshot of either a 1D or 2D grid.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GridState {
    D1 { width: usize, history_limit: usize, cells: Vec<CellState>, step: u64, rule: Rule1D },
    D2 { width: usize, height: usize, history_limit: usize, cells: Vec<CellState>, step: u64, rule: Rule2D },
}

impl GridState {
    pub fn from_grid1d(g: &Grid1D) -> Self { Self::D1 { width: g.width, history_limit: g.history_limit, cells: g.cells.clone(), step: g.step, rule: g.rule.clone() } }
    pub fn from_grid2d(g: &Grid2D) -> Self { Self::D2 { width: g.width, height: g.height, history_limit: g.history_limit, cells: g.cells.clone(), step: g.step, rule: g.rule.clone() } }

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
            GridState::D1 { width, history_limit, cells, step, rule } => Some(Self { width: *width, history_limit: *history_limit, cells: cells.clone(), step: *step, rule: rule.clone() }),
            _ => None,
        }
    }
}

impl Grid2D {
    /// Build a Grid2D from a matching GridState variant.
    pub fn from_state(state: &GridState) -> Option<Self> {
        match state {
            GridState::D2 { width, height, history_limit, cells, step, rule } => Some(Self { width: *width, height: *height, history_limit: *history_limit, cells: cells.clone(), step: *step, rule: rule.clone() }),
            _ => None,
        }
    }
}

/// Back-compat helper kept for examples.
pub fn grid2d_to_json(g: &Grid2D) -> String { GridState::from_grid2d(g).to_json_pretty() }
