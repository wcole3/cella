//! Core types for cella: cell kinds and per-cell state.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;

/// Name used for the implicit inactive/background cell type.
pub const INACTIVE: &str = "Inactive";

/// A semantic label for a cell's type/state.
///
/// Cell types are arbitrary strings and can be used to distinguish
/// living/dead, species, phases, etc. A special built-in type is
/// [`INACTIVE`], representing the background/border.
///
/// Examples
/// ```rust
/// use cella_lib::CellType;
/// let alive = CellType("Alive".into());
/// let inactive = CellType::inactive();
/// assert_ne!(alive, inactive);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellType(pub String);

impl CellType {
    /// Convenience constructor for the inactive/background type.
    ///
    /// ```rust
    /// use cella_lib::{CellType, INACTIVE};
    /// let t = CellType::inactive();
    /// assert_eq!(t.0, INACTIVE);
    ///
    /// let t2 = CellType("testType".into());
    /// assert_ne!(t, t2);
    /// assert_eq!(t2.0, String::from("testType"));
    /// ```
    pub fn inactive() -> Self { CellType(INACTIVE.to_string()) }
}

impl Default for CellType {
    fn default() -> Self { CellType::inactive() }
}

impl fmt::Display for CellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

/// Per-cell state tracked by a grid.
///
/// It includes the current [`CellType`], how long the cell stayed in
/// that state, and a bounded history of previous states.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CellState {
    pub current: CellType,
    /// Number of consecutive steps the cell has been in `current`.
    pub age_in_state: u32,
    /// FIFO of previous states, bounded by `history_limit`.
    pub history: VecDeque<CellType>,
    /// Maximum number of previous states to keep.
    pub history_limit: usize,
}

impl CellState {
    /// Create a new cell with the given `current` type and a history cap.
    ///
    /// ```rust
    /// use cella_lib::{CellState, CellType};
    /// let st = CellState::new(CellType("Alive".into()), 3);
    /// assert_eq!(st.age_in_state, 0);
    /// ```
    pub fn new(current: CellType, history_limit: usize) -> Self {
        Self { current, age_in_state: 0, history: VecDeque::new(), history_limit }
    }

    /// Transition the cell to `next`, updating history and age counters.
    ///
    /// - If `next` equals `current`, only `age_in_state` increases.
    /// - Otherwise, `current` is pushed into `history` (bounded),
    ///   and `age_in_state` resets to 0.
    ///
    /// ```rust
    /// use cella_lib::{CellState, CellType};
    /// let mut st = CellState::new(CellType("A".into()), 2);
    /// st.transition(&CellType("B".into()));
    /// assert_eq!(st.history.len(), 1);
    /// assert_eq!(st.age_in_state, 0);
    /// st.transition(&CellType("B".into()));
    /// assert_eq!(st.age_in_state, 1);
    /// ```
    pub fn transition(&mut self, next: &CellType) {
        if &self.current == next {
            self.age_in_state = self.age_in_state.saturating_add(1);
        } else {
            self.history.push_back(self.current.clone());
            while self.history.len() > self.history_limit {
                self.history.pop_front();
            }
            self.current = next.clone();
            self.age_in_state = 0;
        }
    }
}
