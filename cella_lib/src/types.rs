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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
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
    /// FIFO of previous states, bounded by `history_limit`.
    pub history: VecDeque<CellType>,
    /// Number of consecutive steps the cell has been in `current`.
    pub age_in_state: u32,
    /// Maximum number of previous states to keep.
    pub history_limit: usize,
    /// Current cell type.
    pub current: CellType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_string_and_debug_are_usable() {
        let ct = CellType::from(String::from("DebugType"));
        assert_eq!(ct.as_str(), "DebugType");
        let dbg = format!("{:?}", ct);
        assert!(dbg.contains("CellType"));
        assert!(dbg.contains("DebugType"));
    }

    #[test]
    fn default_and_display_are_usable() {
        let ct = CellType::default();
        assert_eq!(ct, CellType::inactive());
        assert_eq!(format!("{}", ct), INACTIVE);
    }

    #[test]
    fn deserialize_rejects_non_string() {
        let bad = serde_json::from_str::<CellType>("123");
        assert!(bad.is_err());
    }
}

