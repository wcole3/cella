//! 1D grid implementation.
use serde::{Deserialize, Serialize};
use crate::types::{CellState, CellType};
use crate::rules::Rule1D;

/// 1D grid containing cells and a 1D rule.
///
/// Create with [`Grid1D::new`], then call [`Grid1D::step`] repeatedly.
///
/// Example
/// ```rust
/// use cella_lib::{Grid1D, Rule1D, Rule1DSubrule, CellType};
/// let x = CellType("X".into());
/// let rule = Rule1D { subrules: vec![Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() }]};
/// let width = 5usize;
/// let mut init = vec![CellType::inert(); width];
/// init[width/2] = x.clone();
/// let mut g = Grid1D::new(width, 3, init, rule);
/// g.step();
/// assert!(g.step >= 1);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid1D {
    /// Number of cells in the row.
    pub width: usize,
    /// Max number of past states retained for each cell.
    pub history_limit: usize,
    /// Cells stored left-to-right.
    pub cells: Vec<CellState>,
    /// Current simulation step.
    pub step: u64,
    /// Rule used for updates.
    pub rule: Rule1D,
}

impl Grid1D {
    /// Construct a new 1D grid.
    ///
    /// `initial.len()` must equal `width`.
    pub fn new(width: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule1D) -> Self {
        assert_eq!(initial.len(), width, "initial types len must equal width");
        let cells = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        Self { width, history_limit, cells, step: 0, rule }
    }

    fn get_type_or_inert(&self, idx: isize) -> CellType {
        if idx < 0 || idx as usize >= self.width { return CellType::inert(); }
        self.cells[idx as usize].current.clone()
    }

    /// Advance the automaton by one step using double-buffering.
    ///
    /// Evaluates subrules in order; if none trigger, the cell becomes
    /// [`CellType::inert`]. History and ages are updated accordingly.
    pub fn step(&mut self) {
        let mut next = self.cells.clone();
        for i in 0..self.width {
            let current_type = self.cells[i].current.clone();
            let mut decided: Option<CellType> = None;
            'sub: for s in &self.rule.subrules {
                if &current_type != &s.current_type { continue; }
                let n = s.n as isize;
                let len = 2 * n + 1;
                let mut window: Vec<CellType> = Vec::with_capacity(len as usize);
                for d in -n..=n { window.push(self.get_type_or_inert(i as isize + d)); }
                if let Some(out) = s.applies_and_output(&current_type, &window) {
                    decided = Some(out);
                    break 'sub;
                }
            }
            let new_type = decided.unwrap_or_else(CellType::inert);
            next[i].transition(&new_type);
        }
        self.cells = next;
        self.step = self.step.saturating_add(1);
    }
}
