//! 1D grid implementation.
use serde::{Deserialize, Serialize};
use crate::types::{CellState, CellType};
use crate::rules::{Rule1D};

/// 1D grid containing cells and a 1D rule.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid1D {
    pub width: usize,
    pub history_limit: usize,
    pub cells: Vec<CellState>,
    pub step: u64,
    pub rule: Rule1D,
}

impl Grid1D {
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
