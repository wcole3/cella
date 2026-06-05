//! Core types for cella: cell kinds and per-cell state.
use lasso2::{Spur, ThreadedRodeo};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::VecDeque;
use std::fmt;
use std::sync::OnceLock;

/// Name used for the implicit inactive/background cell type.
pub const INACTIVE: &str = "Inactive";

static INTERNER: OnceLock<ThreadedRodeo> = OnceLock::new();
#[inline]
pub fn interner() -> &'static ThreadedRodeo { INTERNER.get_or_init(ThreadedRodeo::default) }

/// A semantic label for a cell's type/state.
///
/// Cell types are arbitrary strings and can be used to distinguish
/// living/dead, species, phases, etc. A special built-in type is
/// [`INACTIVE`], representing the background/border.
///
/// Examples
/// ```rust
/// use cella_lib::CellType;
/// let alive = CellType::from("Alive");
/// let inactive = CellType::inactive();
/// assert_ne!(alive, inactive);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellType(pub Spur);

impl CellType {
    /// Convenience constructor for the inactive/background type.
    ///
    /// ```rust
    /// use cella_lib::{CellType, INACTIVE};
    /// use cella_lib::types::interner;
    /// let t = CellType::inactive();
    /// assert_eq!(t.0, interner().get_or_intern(INACTIVE));
    ///
    /// let t2 = CellType::from("testType");
    /// assert_ne!(t, t2);
    /// assert_eq!(t2.as_str(), String::from("testType"));
    /// ```
    #[inline] pub fn new(name: &str) -> Self{CellType(interner().get_or_intern(name))}
    #[inline] pub fn as_str(&self) -> &'static str { interner().resolve(&self.0) }
    pub fn inactive() -> Self { Self::new(INACTIVE) }
}

// ergo
impl From<&str> for CellType { fn from(s: &str) -> Self { CellType::new(s) }}
impl From<String> for CellType { fn from(s: String) -> Self { CellType::new(&s) }}

impl Default for CellType {
    fn default() -> Self { CellType::inactive() }
}

impl fmt::Display for CellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.as_str()) }
}

impl fmt::Debug for CellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "CellType({:?}", self.as_str()) }
}

impl Serialize for CellType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CellType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(CellType::new(&s))
    }
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
    /// let st = CellState::new(CellType::from("Alive"), 3);
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
    /// let mut st = CellState::new(CellType::from("A"), 2);
    /// st.transition(&CellType::from("B"));
    /// assert_eq!(st.history.len(), 1);
    /// assert_eq!(st.age_in_state, 0);
    /// st.transition(&CellType::from("B"));
    /// assert_eq!(st.age_in_state, 1);
    /// ```
    pub fn transition(&mut self, next: &CellType) {
        if &self.current == next {
            self.age_in_state = self.age_in_state.saturating_add(1);
        } else {
            self.history.push_back(self.current);
            while self.history.len() > self.history_limit {
                self.history.pop_front();
            }
            self.current = *next;
            self.age_in_state = 0;
        }
    }
}
