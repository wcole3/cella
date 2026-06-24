//! 2D grid implementation.

use std::io::Error;
use crate::rules::{Rule2D, TypeCounter};
use crate::threads::thread_count;
use crate::types::{CellState, CellType};
use lasso2::Spur;
use serde::{Deserialize, Deserializer, Serialize};

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
#[derive(Clone, Serialize)]
pub struct Grid2D {
    /// Grid width in cells.
    pub width: usize,
    /// Grid height in cells.
    pub height: usize,
    /// Max number of past states retained for each cell.
    pub history_limit: usize,
    /// Row-major length width*height
    pub cell_states: Vec<CellState>,
    /// double buffer of celltype TODO there might be a more efficient way to combine these with
    /// the CellState array
    #[serde(skip)] pub(crate) cells: Vec<CellType>,
    #[serde(skip)] pub(crate) next_cells: Vec<CellType>,
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
    /// store the type with the highest count so we can skip it during counts
    #[serde(skip)] pub(crate) dominant_type: CellType,
}

impl<'de> Deserialize<'de> for Grid2D {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // deserialize into an intermediate struct without the skipped fields
        #[derive(Deserialize)]
        struct Grid2DIntermediate {
            width: usize,
            height: usize,
            history_limit: usize,
            cell_states: Vec<CellState>,
            step: u64,
            rule: Rule2D,
            counts_current: std::collections::HashMap<Spur, u64>,
            peak_counts: std::collections::HashMap<Spur, u64>,
            inactive: CellType,
        }
        let intermediate = Grid2DIntermediate::deserialize(d)?;
        // reconstruct the skipped fields
        let cells: Vec<CellType> = intermediate.cell_states.iter().map(|cs| cs.current).collect();
        let next_cells: Vec<CellType> = vec![intermediate.inactive.clone(); intermediate.width * intermediate.height];
        // find max count celltype
        let dominant_type: CellType = intermediate.counts_current.iter()
            .max_by_key(|entry| entry.1)
            .map(|(spur, _)| CellType(*spur))
            .unwrap_or_else(|| intermediate.inactive.clone());
        Ok(Grid2D {
            width: intermediate.width,
            height: intermediate.height,
            history_limit: intermediate.history_limit,
            cell_states: intermediate.cell_states,
            cells,
            next_cells,
            step: intermediate.step,
            rule: intermediate.rule,
            counts_current: intermediate.counts_current,
            peak_counts: intermediate.peak_counts,
            inactive: intermediate.inactive,
            dominant_type,
        })
    }
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
        let cell_states: Vec<CellState> = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        let next_cells: Vec<CellType> = vec![CellType::inactive(); width * height];
        let cells: Vec<CellType> = cell_states.iter().map(|cs| cs.current).collect();
        let mut counts_current: std::collections::HashMap<Spur, u64> = std::collections::HashMap::new();
        let mut dominant_type: (CellType, u64) = (CellType::inactive(), 0);
        for c in &cells {
            *counts_current.entry(c.0).or_insert(0) += 1;
            if counts_current.entry(c.0).or_insert(0) > &mut dominant_type.1 {
                dominant_type = (c.clone(), *counts_current.entry(c.0).or_insert(0));
            }
        }
        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self { width, height, history_limit, cell_states, cells,
            next_cells, step: 0, rule, counts_current, peak_counts, inactive, dominant_type: dominant_type.0}
    }

    /// Transitions the given CellState and current buffer cell type
    /// Used by interactive or programatic routines that change grid
    /// state outside of stepping (i.e. grid painting)
    pub fn transition_state_and_buffer(&mut self, idx: usize, new_type: &CellType) -> Option<Error> {
        // validate the idx if outside return error
        if idx > (self.width * self.height) {
            Some(Error::new(std::io::ErrorKind::InvalidInput, "Index out of bounds"))
        }
        else {
            self.cell_states[idx].transition(new_type);
            self.cells[idx] = *new_type;
            None
        }
    }

    fn recompute_counts_from_cells(&mut self, new_counts: &TypeCounter) {
        // clear the current counts
        self.counts_current.clear();
        let mut total_count: u64 = self.cells.len() as u64;
        let mut new_dominant_type: (&CellType, u64) = (&CellType::inactive(), 0);
        for (k, v) in new_counts.iter() {
            self.counts_current.entry(k.0).or_insert(*v);
            total_count -= *v;
            // check if peak count needs to be updated
            self.peak_counts.entry(k.0).and_modify(|count| {
                if *count < *v {
                    *count = *v;
                }
            }).or_insert(*v);
            if *v >= new_dominant_type.1 {
                new_dominant_type.0 = k
            }
        }
        // add the dominant type
        self.counts_current.entry(self.dominant_type.0).or_insert(total_count);
        self.peak_counts.entry(self.dominant_type.0).and_modify(|count| {
                if *count < total_count {
                    *count = total_count;
                }
            }).or_insert(total_count);
        if total_count < new_dominant_type.1 {
            self.dominant_type = *new_dominant_type.0;
        }
    }

    /// Type at `(x, y)`, or [`inactive`](Self::inactive) when out of bounds.
    ///
    /// Reads from the `cells` slice directly so it can be shared by the serial
    /// and parallel paths (which cannot borrow `&self`).
    #[inline]
    fn neighbor(cells: &[CellType], inactive: &CellType, width: usize, height: usize, x: isize, y: isize) -> CellType {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
            *inactive
        } else {
            cells[y as usize * width + x as usize]
        }
    }

    /// Evaluate subrules in order for the cell at `(x, y)`, returning its next type.
    ///
    /// Subrules are tried in order; the first that applies wins. If none trigger
    /// the cell becomes [`inactive`](Self::inactive).
    #[inline]
    fn next_type<'a>(cells: &'a [CellType], rule: &'a Rule2D, inactive: &'a CellType,
                      width: usize, height: usize, x: isize, y: isize) -> &'a CellType {
        let idx = y as usize * width + x as usize;
        let current_type = &cells[idx];
        for sr in &rule.subrules {
            if current_type != &sr.current_type { continue; }
            let out = sr.applies_and_output(current_type, |dx, dy| {
                Self::neighbor(cells, inactive, width, height, x + dx as isize, y + dy as isize)
            });
            if let Some(o) = out { return o; }
        }
        inactive
    }

    /// Compute the next state for global indices `start..start + next_cells.len()`,
    /// writing into the chunk-local `next_cells` / `cell_states` slices.
    ///
    /// `cells` is the full current grid (neighbor lookups span chunk boundaries);
    /// `next_cells` and `cell_states` are this chunk's disjoint output slices.
    fn step_chunk(cells: &[CellType], next_cells: &mut [CellType], cell_states: &mut [CellState],
                   rule: &Rule2D, inactive: &CellType, dt: &CellType,
                   width: usize, height: usize, start: usize) -> TypeCounter {
        let mut count_map = TypeCounter::new();
        let mut x = (start % width) as isize;
        let mut y = (start / width) as isize;
        for local in 0..next_cells.len() {
            let new_type = *Self::next_type(cells, rule, inactive, width, height, x, y);
            next_cells[local] = new_type;
            cell_states[local].transition(&new_type);
            if new_type != *dt {
                count_map.add(new_type);
            }
            x += 1;
            if x == width as isize {
                x = 0;
                y += 1;
            }
        }
        count_map
    }

    /// Advance the automaton by one step using double-buffering.
    ///
    /// Evaluates subrules in order; if none trigger, the cell becomes
    /// [`CellType::inactive`]. History and ages are updated accordingly.
    /// May run in parallel depending on the `threads` setting in
    /// `cella.properties` at the repository root.
    pub fn step(&mut self) {
        let threads = thread_count();
        let width = self.width;
        let height = self.height;
        let total = width * height;

        // Disjoint field borrows shared by both paths; threads can't borrow `&self`.
        let cells = &self.cells;
        let next_cells = &mut self.next_cells;
        let cell_states = &mut self.cell_states;
        let rule = &self.rule;
        let inactive = &self.inactive;
        let dt = &self.dominant_type;

        let count_map = if threads <= 1 || total < 4096 {
            Self::step_chunk(cells, next_cells, cell_states, rule, inactive, dt, width, height, 0)
        } else {
            // One thread per chunk; `chunks_mut` hands each thread a disjoint
            // mutable slice, so the writes are sound without locking.
            let chunk = (total + threads - 1) / threads;
            std::thread::scope(|s| {
                let handles: Vec<_> = (0..)
                    .map(|t| t * chunk)
                    .zip(next_cells.chunks_mut(chunk))
                    .zip(cell_states.chunks_mut(chunk))
                    .map(|((start, next_chunk), state_chunk)| {
                        s.spawn(move || {
                            Self::step_chunk(cells, next_chunk, state_chunk, rule, inactive, dt, width, height, start)
                        })
                    })
                    .collect();

                let mut merged = TypeCounter::new();
                for handle in handles {
                    let chunk_counter = handle.join().unwrap();
                    merged.merge(&chunk_counter);
                }
                merged
            })
        };

        // swap the buffers
        std::mem::swap(&mut self.cells, &mut self.next_cells);

        // Update counts and peaks.
        Self::recompute_counts_from_cells(self, &count_map);
        self.step = self.step.saturating_add(1);
    }
}
