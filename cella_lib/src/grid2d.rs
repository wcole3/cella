//! 2D grid implementation.
use serde::{Deserialize, Serialize};
use crate::types::{CellState, CellType};
use crate::rules::Rule2D;
use crate::threads::thread_count;

/// 2D grid containing cells and a 2D rule.
///
/// Create with [`Grid2D::new`], then call [`Grid2D::step`] repeatedly.
/// Cells are stored row-major in `cells` with length `width*height`.
///
/// Example
/// ```rust
/// use cella_lib::{Grid2D, Rule2D, Rule2DSubrule, Neighborhood2D, CellType};
/// let alive = CellType("Alive".into());
/// let inactive = CellType::inactive();
/// let rule = Rule2D { subrules: vec![
///   // Overpopulation: Alive with 4+ Alive neighbors becomes Inactive
///   Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 4, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: inactive.clone() },
///   // Survival: Alive stays Alive if at least 2 Alive neighbors (after overpop check)
///   Rule2DSubrule { current_type: alive.clone(), criteria_type: alive.clone(), count: 2, op: CountOp::Gt, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
///   // Birth: Inactive becomes Alive if exactly 3 Alive neighbors
///   Rule2DSubrule { current_type: inactive.clone(),  criteria_type: alive.clone(), count: 3, op: CountOp::Eq, limit: None, range: 1, neighborhood: Neighborhood2D::Moore, randomness: None, output_type: alive.clone() },
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
}

impl Grid2D {
    /// Construct a new 2D grid.
    ///
    /// `initial.len()` must equal `width*height`.
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

    fn get_type_or_inactive(&self, x: isize, y: isize) -> CellType {
        match self.idx(x, y) { Some(i) => self.cells[i].current.clone(), None => CellType::inactive() }
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
        let total = self.width * self.height;
        if threads <= 1 || total < 4096 {
            for y in 0..self.height {
                for x in 0..self.width {
                    let i = y * self.width + x;
                    let current_type = self.cells[i].current.clone();
                    let mut decided: Option<CellType> = None;
                    'sub: for s in &self.rule.subrules {
                        if &current_type != &s.current_type { continue; }
                        let out = s.applies_and_output(&current_type, |dx, dy| {
                            self.get_type_or_inactive(x as isize + dx as isize, y as isize + dy as isize)
                        });
                        if let Some(o) = out { decided = Some(o); break 'sub; }
                    }
                    let new_type = decided.unwrap_or_else(CellType::inactive);
                    next[i].transition(&new_type);
                }
            }
            self.cells = next;
            self.step = self.step.saturating_add(1);
            return;
        }
        // Parallel path: snapshot, compute outputs per linear index in chunks
        let snapshot = self.cells.clone();
        let rule = self.rule.clone();
        let width = self.width;
        let height = self.height;
        let chunk = (total + threads - 1) / threads;
        let mut handles = Vec::new();
        for t in 0..threads {
            let start = t * chunk;
            if start >= total { break; }
            let end = ((t + 1) * chunk).min(total);
            let snapshot_t = snapshot.clone();
            let rule_t = rule.clone();
            handles.push(std::thread::spawn(move || {
                let mut out: Vec<(usize, CellType)> = Vec::with_capacity(end - start);
                for idx in start..end {
                    let y = idx / width;
                    let x = idx % width;
                    let current_type = snapshot_t[idx].current.clone();
                    let mut decided: Option<CellType> = None;
                    'sub: for s in &rule_t.subrules {
                        if &current_type != &s.current_type { continue; }
                        let out_ty = s.applies_and_output(&current_type, |dx, dy| {
                            let nx = x as isize + dx as isize;
                            let ny = y as isize + dy as isize;
                            if nx < 0 || ny < 0 || (nx as usize) >= width || (ny as usize) >= height {
                                CellType::inactive()
                            } else {
                                snapshot_t[(ny as usize) * width + (nx as usize)].current.clone()
                            }
                        });
                        if let Some(o) = out_ty { decided = Some(o); break 'sub; }
                    }
                    let new_type = decided.unwrap_or_else(CellType::inactive);
                    out.push((idx, new_type));
                }
                out
            }));
        }
        for h in handles { for (idx, ty) in h.join().expect("thread join") { next[idx].transition(&ty); } }
        self.cells = next;
        self.step = self.step.saturating_add(1);
    }
}
