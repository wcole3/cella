//! 2D grid implementation.
use serde::{Deserialize, Serialize};
use crate::types::{CellState, CellType};
use crate::rules::Rule2D;

/// 2D grid containing cells and a 2D rule.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid2D {
    pub width: usize,
    pub height: usize,
    pub history_limit: usize,
    /// Row-major length width*height
    pub cells: Vec<CellState>,
    pub step: u64,
    pub rule: Rule2D,
}

impl Grid2D {
    pub fn new(width: usize, height: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule2D) -> Self {
        assert_eq!(initial.len(), width * height, "initial types len must equal width*height");
        let cells = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        Self { width, height, history_limit, cells, step: 0, rule }
    }

    fn idx(&self, x: isize, y: isize) -> Option<usize> {
        if x < 0 || y < 0 { return None; }
        let (xu, yu) = (x as usize, y as usize);
        if xu >= self.width || yu >= self.height { return None; }
        Some(yu * self.width + xu)
    }

    fn get_type_or_inert(&self, x: isize, y: isize) -> CellType {
        match self.idx(x, y) { Some(i) => self.cells[i].current.clone(), None => CellType::inert() }
    }

    /// Advance the automaton by one step using double-buffering.
    pub fn step(&mut self) {
        let mut next = self.cells.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let i = y * self.width + x;
                let current_type = self.cells[i].current.clone();
                let mut decided: Option<CellType> = None;
                'sub: for s in &self.rule.subrules {
                    if &current_type != &s.current_type { continue; }
                    let out = s.applies_and_output(&current_type, |dx, dy| {
                        self.get_type_or_inert(x as isize + dx as isize, y as isize + dy as isize)
                    });
                    if let Some(o) = out { decided = Some(o); break 'sub; }
                }
                let new_type = decided.unwrap_or_else(CellType::inert);
                next[i].transition(&new_type);
            }
        }
        self.cells = next;
        self.step = self.step.saturating_add(1);
    }
}
