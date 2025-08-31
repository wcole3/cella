//! Core types for cella: cell kinds and per-cell state.
use serde::{Deserialize, Serialize};
use std::fmt;

/// Name used for the implicit inert/background cell type.
pub const INERT: &str = "Inert";

/// A semantic label for a cell's type/state.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellType(pub String);

impl CellType {
    /// Convenience constructor for the inert/background type.
    pub fn inert() -> Self { CellType(INERT.to_string()) }
}

impl Default for CellType {
    fn default() -> Self { CellType::inert() }
}

impl fmt::Display for CellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

/// Per-cell state tracked by a grid.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CellState {
    pub current: CellType,
    pub age_in_state: u32,
    pub history: Vec<CellType>,
    pub history_limit: usize,
}

impl CellState {
    pub fn new(current: CellType, history_limit: usize) -> Self {
        Self { current, age_in_state: 0, history: Vec::new(), history_limit }
    }

    /// Transition the cell to `next`, updating history and age counters.
    pub fn transition(&mut self, next: &CellType) {
        if &self.current == next {
            self.age_in_state = self.age_in_state.saturating_add(1);
        } else {
            self.history.push(self.current.clone());
            if self.history.len() > self.history_limit {
                let remove_n = self.history.len() - self.history_limit;
                self.history.drain(0..remove_n);
            }
            self.current = next.clone();
            self.age_in_state = 0;
        }
    }
}
