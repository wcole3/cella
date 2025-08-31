//! 1D grid implementation.
use serde::{Deserialize, Serialize};
use crate::types::{CellState, CellType};
use crate::rules::Rule1D;
use crate::threads::thread_count;

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
/// let mut init = vec![CellType::inactive(); width];
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

    fn get_type_or_inactive(&self, idx: isize) -> CellType {
        if idx < 0 || idx as usize >= self.width { return CellType::inactive(); }
        self.cells[idx as usize].current.clone()
    }

    /// Advance the automaton by one step using double-buffering.
    ///
    /// Evaluates subrules in order; if none trigger, the cell becomes
    /// [`CellType::inactive`]. History and ages are updated accordingly.
    /// May run in parallel depending on the `threads` setting in
    /// `cella.properties` at the repository root.
    pub fn step(&mut self) {
        let mut next = self.cells.clone();
        let threads = thread_count();
        // Serial fallback for small grids or single-thread config
        if threads <= 1 || self.width < 256 {
            for i in 0..self.width {
                let current_type = self.cells[i].current.clone();
                let mut decided: Option<CellType> = None;
                'sub: for s in &self.rule.subrules {
                    if &current_type != &s.current_type { continue; }
                    let n = s.n as isize;
                    let len = 2 * n + 1;
                    let mut window: Vec<CellType> = Vec::with_capacity(len as usize);
                    for d in -n..=n { window.push(self.get_type_or_inactive(i as isize + d)); }
                    if let Some(out) = s.applies_and_output(&current_type, &window) {
                        decided = Some(out);
                        break 'sub;
                    }
                }
                let new_type = decided.unwrap_or_else(CellType::inactive);
                next[i].transition(&new_type);
            }
            self.cells = next;
            self.step = self.step.saturating_add(1);
            return;
        }
        // Parallel path: snapshot immutable inputs, compute outputs per index
        let snapshot = self.cells.clone();
        let rule = self.rule.clone();
        let width = self.width;
        let chunk = (width + threads - 1) / threads;
        let mut handles = Vec::new();
        for t in 0..threads {
            let start = t * chunk;
            if start >= width { break; }
            let end = ((t + 1) * chunk).min(width);
            let snapshot_t = snapshot.clone();
            let rule_t = rule.clone();
            handles.push(std::thread::spawn(move || {
                let mut out: Vec<(usize, CellType)> = Vec::with_capacity(end - start);
                for i in start..end {
                    let current_type = snapshot_t[i].current.clone();
                    let mut decided: Option<CellType> = None;
                    'sub: for s in &rule_t.subrules {
                        if &current_type != &s.current_type { continue; }
                        let n = s.n as isize;
                        let len = 2 * n + 1;
                        let mut window: Vec<CellType> = Vec::with_capacity(len as usize);
                        for d in -n..=n {
                            let idx = i as isize + d;
                            if idx < 0 || (idx as usize) >= width { window.push(CellType::inactive()); }
                            else { window.push(snapshot_t[idx as usize].current.clone()); }
                        }
                        if let Some(o) = s.applies_and_output(&current_type, &window) {
                            decided = Some(o);
                            break 'sub;
                        }
                    }
                    let new_type = decided.unwrap_or_else(CellType::inactive);
                    out.push((i, new_type));
                }
                out
            }));
        }
        for h in handles {
            for (i, ty) in h.join().expect("thread join") { next[i].transition(&ty); }
        }
        self.cells = next;
        self.step = self.step.saturating_add(1);
    }
}
