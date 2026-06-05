//! 1D grid implementation.

use lasso2::Spur;
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
/// let x = CellType::from("X");
/// let rule = Rule1D { subrules: vec![Rule1DSubrule { current_type: x.clone(), criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() }]};
/// let width = 5usize;
/// let mut init = vec![CellType::inactive(); width];
/// init[width/2] = x.clone();
/// let mut g = Grid1D::new(width, 3, init, rule);
/// g.step();
/// assert!(g.step >= 1);
/// ```
#[derive(Clone, Serialize, Deserialize)]
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
    /// Current count of cells per type name.
    pub counts_current: std::collections::HashMap<Spur, u64>,
    /// Peak (max-so-far) count of cells per type name since start/reset.
    pub peak_counts: std::collections::HashMap<Spur, u64>,
    /// Reference to inactive cell type.
    pub inactive: CellType,
}

impl std::fmt::Debug for Grid1D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grid1D")
            .field("width", &self.width)
            .field("history_limit", &self.history_limit)
            .field("cells", &self.cells)
            .field("step", &self.step)
            .field("rule", &self.rule)
            .field("counts_current", &self.counts_current)
            .field("peak_counts", &self.peak_counts)
            .finish()
    }
}

impl Grid1D {
    /// Construct a new 1D grid.
    ///
    /// `initial.len()` must equal `width`.
    pub fn new(width: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule1D) -> Self {
        assert_eq!(initial.len(), width, "initial types len must equal width");
        let cells: Vec<CellState> = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        let mut counts_current: std::collections::HashMap<Spur, u64> = std::collections::HashMap::new();
        for c in &cells { *counts_current.entry(c.current.0).or_insert(0) += 1; }
        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self { width, history_limit, cells, step: 0, rule, counts_current, peak_counts, inactive }
    }

    fn get_type_or_inactive(&self, idx: isize) -> &CellType {
        if idx < 0 || idx as usize >= self.width { return &self.inactive; }
        &self.cells[idx as usize].current
    }

    fn recompute_counts_from_cells(&mut self) {
        for c in &self.cells {
            *self.counts_current.entry(c.current.0).or_insert(0) += 1;
            *self.peak_counts.entry(c.current.0).or_insert(0) += 1;
        }
    }

    /// Advance the automaton by one step using double-buffering.
    ///
    /// Evaluates subrules in order; if none trigger, the cell becomes
    /// [`CellType::inactive`]. History and ages are updated accordingly.
    /// May run in parallel depending on the `threads` setting in
    /// `cella.properties` at the repository root.
    pub fn step(&mut self) {
        let threads = thread_count();
        let mut next: Vec<&CellType> = Vec::with_capacity(self.width);
        
        // Serial fallback — 1D per-cell work is very cheap (small window +
        // bit-check), so threading overhead dominates for anything but very
        // large grids.  Require at least 8192 cells before spawning threads.
        if threads <= 1 || self.width < 8192 {
            for i in 0..self.width {
                let current_type = &self.cells[i].current;
                let mut decided: Option<&CellType> = None;
                'sub: for s in &self.rule.subrules {
                    if current_type != &s.current_type { continue; }
                    let n = s.n as isize;
                    let len = 2 * n + 1;
                    let mut window: Vec<CellType> = Vec::with_capacity(len as usize);
                    for d in -n..=n { window.push(*self.get_type_or_inactive(i as isize + d)); }
                    if let Some(out) = s.applies_and_output(current_type, &window) {
                        decided = Some(out);
                        break 'sub;
                    }
                }
                let new_type = decided.unwrap_or_else(|| &self.inactive);
                next.push(new_type);
            }
            // transition all cells
            for (i, ty) in next.drain(..).enumerate() {
                self.cells[i].transition(&ty);
            }
        } else {
            // Parallel path using scoped threads — borrows cells & rule directly
            let cells = &self.cells;
            let rule = &self.rule;
            let width = self.width;
            let chunk = (width + threads - 1) / threads;
            let inactive = &self.inactive;

            let all_results: Vec<Vec<(usize, &CellType)>> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..threads)
                    .filter_map(|t| {
                        let start = t * chunk;
                        if start >= width { return None; }
                        let end = ((t + 1) * chunk).min(width);
                        Some(s.spawn(move || {
                            let mut out: Vec<(usize, &CellType)> = Vec::with_capacity(end - start);
                            for i in start..end {
                                let current_type = &cells[i].current;
                                let mut decided: Option<&CellType> = None;
                                'sub: for sr in &rule.subrules {
                                    if current_type != &sr.current_type { continue; }
                                    let n = sr.n as isize;
                                    let len = 2 * n + 1;
                                    let mut window: Vec<CellType> = Vec::with_capacity(len as usize);
                                    for d in -n..=n {
                                        let idx = i as isize + d;
                                        if idx < 0 || (idx as usize) >= width { window.push(*inactive); }
                                        else { window.push(cells[idx as usize].current); }
                                    }
                                    if let Some(o) = sr.applies_and_output(current_type, &window) {
                                        decided = Some(o);
                                        break 'sub;
                                    }
                                }
                                let new_type = decided.unwrap_or_else(|| &inactive);
                                out.push((i, new_type));
                            }
                            out
                        }))
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });

            for chunk_results in all_results {
                for (i, ty) in chunk_results { self.cells[i].transition(&ty); }
            }
        }

        // Update counts and peaks
        Self::recompute_counts_from_cells(self);
        self.step = self.step.saturating_add(1);
    }
}
