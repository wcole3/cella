//! 2D grid implementation.

use crate::rules::Rule2D;
use crate::threads::thread_count;
use crate::types::{CellState, CellType};
use lasso2::Spur;
use serde::{Deserialize, Serialize};

/// 2D grid containing cells and a 2D rule.
///
/// Create with [`Grid2D::new`], then call [`Grid2D::step`] repeatedly.
/// Cells are stored row-major in `cells` with length `width*height`.
///
/// Example
/// ```rust
/// use cella_lib::{Grid2D, Rule2D, Rule2DSubrule, Neighborhood2D, CellType, CountOp};
/// let alive = CellType::from("Alive");
/// let inactive = CellType::inactive();
/// let rule = Rule2D { subrules: vec![
///   // Overpopulation: Alive with 4+ Alive neighbors becomes Inactive
///   Rule2DSubrule::new(alive.clone(), alive.clone(), 4, CountOp::Gt, 1, Neighborhood2D::Moore, inactive.clone(), None, None),
///   // Survival: Alive stays Alive if at least 2 Alive neighbors (after overpop check)
///   Rule2DSubrule::new(alive.clone(), alive.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, alive.clone(), None, None),
///   // Birth: Inactive becomes Alive if exactly 3 Alive neighbors
///   Rule2DSubrule::new(inactive.clone(), alive.clone(), 3, CountOp::Eq, 1, Neighborhood2D::Moore, alive.clone(), None, None),
/// ]};
/// let (w,h) = (6usize, 5usize);
/// let mut init = vec![CellType::inactive(); w*h];
/// init[2*w + 2] = alive.clone();
/// init[2*w + 3] = alive.clone();
/// init[2*w + 4] = alive.clone();
/// let mut g = Grid2D::new(w, h, 3, init, rule);
/// g.step();
/// assert!(g.step >= 1);
/// ```
#[derive(Clone, Serialize, Deserialize)]
pub struct Grid2D {
    /// Grid width in cells.
    pub width: usize,
    /// Grid height in cells.
    pub height: usize,
    /// Max number of past states retained for each cell.
    pub history_limit: usize,
    /// Row-major length width*height
    pub cells: Vec<CellState>,
    /// Current simulation step.
    pub step: u64,
    /// Rule used for updates.
    pub rule: Rule2D,
    /// Current count of cells per type name.
    pub counts_current: std::collections::HashMap<Spur, u64>,
    /// Peak (max-so-far) count of cells per type name since start/reset.
    pub peak_counts: std::collections::HashMap<Spur, u64>,
    /// Reference to inactive cell type.
    pub inactive: CellType,
}

impl std::fmt::Debug for Grid2D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grid2D")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("history_limit", &self.history_limit)
            .field("cells", &self.cells)
            .field("step", &self.step)
            .field("rule", &self.rule)
            .field("counts_current", &self.counts_current)
            .field("peak_counts", &self.peak_counts)
            .finish()
    }
}

impl Grid2D {
    /// Construct a new 2D grid.
    ///
    /// `initial.len()` must equal `width*height`.
    pub fn new(width: usize, height: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule2D) -> Self {
        assert_eq!(initial.len(), width * height, "initial types len must equal width*height");
        let cells: Vec<CellState> = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        let mut counts_current: std::collections::HashMap<Spur, u64> = std::collections::HashMap::new();
        for c in &cells { *counts_current.entry(c.current.0).or_insert(0) += 1; }
        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self { width, height, history_limit, cells, step: 0, rule, counts_current, peak_counts, inactive }
    }

    fn idx(&self, x: isize, y: isize) -> Option<usize> {
        if x < 0 || y < 0 { return None; }
        let (xu, yu) = (x as usize, y as usize);
        if xu >= self.width || yu >= self.height { return None; }
        Some(yu * self.width + xu)
    }

    fn get_type_or_inactive(&self, x: isize, y: isize) -> &CellType {
        match self.idx(x, y) { Some(i) => &self.cells[i].current, None => &self.inactive }
    }

    fn recompute_counts_from_cells(&mut self) {
        // clear the current counts
        self.counts_current.clear();
        for c in &self.cells {
            *self.counts_current.entry(c.current.0).or_insert(0) += 1;
            // TODO below this is not correct
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
        let mut next: Vec<&CellType> = Vec::with_capacity(self.width * self.height);
        let threads = thread_count();
        let total = self.width * self.height;

        if threads <= 1 || total < 4096 {
            for y in 0..self.height {
                for x in 0..self.width {
                    let i = y * self.width + x;
                    let current_type = &self.cells[i].current;
                    let mut decided: Option<&CellType> = None;
                    'sub: for s in &self.rule.subrules {
                        if current_type != &s.current_type { continue; }
                        let out = s.applies_and_output(current_type, |dx, dy| {
                            *self.get_type_or_inactive(x as isize + dx as isize, y as isize + dy as isize)
                        });
                        if let Some(o) = out { decided = Some(o); break 'sub; }
                    }
                    let new_type = decided.unwrap_or_else(|| &self.inactive);
                    next.push(new_type);
                }
            }
            for (i, ty) in next.drain(..).enumerate() {
                self.cells[i].transition(&ty);
            }
        } else {
            // Parallel path using scoped threads — borrows cells & rule directly
            let cells = &self.cells;
            let rule = &self.rule;
            let width = self.width;
            let height = self.height;
            let chunk = (total + threads - 1) / threads;
            let inactive = &self.inactive;

            let all_results: Vec<Vec<(usize, &CellType)>> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..threads)
                    .filter_map(|t| {
                        let start = t * chunk;
                        if start >= total { return None; }
                        let end = ((t + 1) * chunk).min(total);
                        Some(s.spawn(move || {
                            let mut out: Vec<(usize, &CellType)> = Vec::with_capacity(end - start);
                            for idx in start..end {
                                let y = idx / width;
                                let x = idx % width;
                                let current_type = &cells[idx].current;
                                let mut decided: Option<&CellType> = None;
                                'sub: for sr in &rule.subrules {
                                    if current_type != &sr.current_type { continue; }
                                    let out_ty = sr.applies_and_output(current_type, |dx, dy| {
                                        let nx = x as isize + dx as isize;
                                        let ny = y as isize + dy as isize;
                                        if nx < 0 || ny < 0 || (nx as usize) >= width || (ny as usize) >= height {
                                            *inactive
                                        } else {
                                            cells[(ny as usize) * width + (nx as usize)].current
                                        }
                                    });
                                    if let Some(o) = out_ty { decided = Some(o); break 'sub; }
                                }
                                let new_type = decided.unwrap_or_else(|| &inactive);
                                out.push((idx, new_type));
                            }
                            out
                        }))
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });

            for chunk_results in all_results {
                for (idx, ty) in chunk_results {
                    self.cells[idx].transition(&ty);
                }
            }
        }

        // Update counts and peaks
        Self::recompute_counts_from_cells(self);
        self.step = self.step.saturating_add(1);
    }
}
