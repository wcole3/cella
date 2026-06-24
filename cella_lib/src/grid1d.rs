//! 1D grid implementation.

use std::io::Error;
use lasso2::Spur;
use serde::{Deserialize, Deserializer, Serialize};
use crate::types::{CellState, CellType};
use crate::rules::{Rule1D, TypeCounter};
use crate::threads::thread_count;
/// 1D grid containing cells and a 1D rule.
///
/// Create with [`Grid1D::new`], then call [`Grid1D::step`] repeatedly.
///
/// Example
/// ```rust
/// use cella_lib::{Grid1D, Rule1D, Rule1DSubrule, CellType};
/// let x = CellType::from("X");
/// let rule = Rule1D { subrules: vec![Rule1DSubrule { current_type: x.clone(),
/// criteria_type: x.clone(), wolfram_code: 30, n: 1, randomness: None, output_type: x.clone() }]};
/// let width = 5usize;
/// let mut init = vec![CellType::inactive(); width];
/// init[width/2] = x.clone();
/// let mut g = Grid1D::new(width, 3, init, rule);
/// g.step();
/// assert!(g.step >= 1);
/// ```
#[derive(Clone, Serialize)]
pub struct Grid1D {
    /// Number of cells in the row.
    pub width: usize,
    /// Max number of past states retained for each cell.
    pub history_limit: usize,
    /// Full cell state (history, age) stored left-to-right.
    pub cell_states: Vec<CellState>,
    /// Current cell types for fast read-only neighbor lookups.
    #[serde(skip)] pub(crate) cells: Vec<CellType>,
    /// Next-step type buffer (double buffer).
    #[serde(skip)] pub(crate) next_cells: Vec<CellType>,
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
    /// store the type with the highest count so we can skip it during counts
    #[serde(skip)] pub(crate) dominant_type: CellType,
}
impl<'de> Deserialize<'de> for Grid1D {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Deserialize into an intermediate struct that excludes the skipped fields.
        #[derive(Deserialize)]
        struct Grid1DIntermediate {
            width: usize,
            history_limit: usize,
            cell_states: Vec<CellState>,
            step: u64,
            rule: Rule1D,
            counts_current: std::collections::HashMap<Spur, u64>,
            peak_counts: std::collections::HashMap<Spur, u64>,
            inactive: CellType,
        }
        let im = Grid1DIntermediate::deserialize(d)?;
        // Reconstruct the skipped buffers from cell_states.
        let cells: Vec<CellType> = im.cell_states.iter().map(|cs| cs.current).collect();
        let next_cells: Vec<CellType> = vec![im.inactive; im.width];
        // find max count celltype
        let dominant_type: CellType = im.counts_current.iter()
            .max_by_key(|entry| entry.1)
            .map(|(spur, _)| CellType(*spur))
            .unwrap_or_else(|| im.inactive.clone());
        Ok(Grid1D {
            width: im.width,
            history_limit: im.history_limit,
            cell_states: im.cell_states,
            cells,
            next_cells,
            step: im.step,
            rule: im.rule,
            counts_current: im.counts_current,
            peak_counts: im.peak_counts,
            inactive: im.inactive,
            dominant_type,
        })
    }
}
impl std::fmt::Debug for Grid1D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grid1D")
            .field("width", &self.width)
            .field("history_limit", &self.history_limit)
            .field("cell_states", &self.cell_states)
            .field("step", &self.step)
            .field("rule", &self.rule)
            .field("counts_current", &self.counts_current)
            .field("peak_counts", &self.peak_counts)
            .field("inactive", &self.inactive)
            .field("dominant_type", &self.dominant_type)
            .finish()
    }
}
impl Grid1D {
    /// Construct a new 1D grid.
    ///
    /// `initial.len()` must equal `width`.
    pub fn new(width: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule1D) -> Self {
        assert_eq!(initial.len(), width, "initial types len must equal width");
        let cell_states: Vec<CellState> = initial.into_iter().map(|t| CellState::new(t, history_limit)).collect();
        let cells: Vec<CellType> = cell_states.iter().map(|cs| cs.current).collect();
        let next_cells: Vec<CellType> = vec![CellType::inactive(); width];
        let mut counts_current: std::collections::HashMap<Spur, u64> = std::collections::HashMap::new();
        for c in &cells { *counts_current.entry(c.0).or_insert(0) += 1; }
        
        let dominant_type = counts_current.iter()
            .max_by_key(|entry| entry.1)
            .map(|(spur, _)| CellType(*spur))
            .unwrap_or_else(|| CellType::inactive());

        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self { width, history_limit, cell_states, cells, next_cells, step: 0, rule, counts_current, peak_counts, inactive, dominant_type }
    }

    /// Transitions the given CellState and current buffer cell type
    /// Used by interactive or programatic routines that change grid
    /// state outside of stepping (i.e. grid painting)
    pub fn transition_state_and_buffer(&mut self, idx: usize, new_type: &CellType) -> Option<Error> {
        // validate the idx if outside return error
        if idx > (self.width) {
            Some(Error::new(std::io::ErrorKind::InvalidInput, "Index out of bounds"))
        }
        else {
            self.cell_states[idx].transition(new_type);
            self.cells[idx] = *new_type;
            None
        }
    }
    
    /// Type at `idx`, or `inactive` when out of bounds.
    ///
    /// Reads from the flat `cells` slice so it can be shared by serial and
    /// parallel paths that cannot borrow `&self`.
    #[inline]
    fn get_type_or_inactive(cells: &[CellType], inactive: &CellType, width: usize, idx: isize) -> CellType {
        if idx < 0 || idx as usize >= width { *inactive } else { cells[idx as usize] }
    }
    /// Evaluate subrules in order for the cell at `idx`, returning its next type.
    ///
    /// Subrules are tried in order; the first that applies wins. If none trigger
    /// the cell becomes [`inactive`](Self::inactive).
    #[inline]
    fn next_type<'a>(cells: &[CellType], rule: &'a Rule1D, inactive: &'a CellType,
                     width: usize, idx: usize) -> &'a CellType {
        let current_type = &cells[idx];
        for s in &rule.subrules {
            if current_type != &s.current_type { continue; }
            let n = s.n as isize;
            let len = (2 * n + 1) as usize;
            let mut window: Vec<CellType> = Vec::with_capacity(len);
            for d in -n..=n {
                window.push(Self::get_type_or_inactive(cells, inactive, width, idx as isize + d));
            }
            if let Some(out) = s.applies_and_output(current_type, &window) {
                return out;
            }
        }
        inactive
    }
    /// Compute the next state for global indices `start..start + next_cells.len()`,
    /// writing into the chunk-local `next_cells` / `cell_states` slices.
    ///
    /// `cells` is the full current grid (neighbor lookups may span chunk boundaries);
    /// `next_cells` and `cell_states` are this chunk's disjoint output slices.
    fn step_chunk(cells: &[CellType], next_cells: &mut [CellType], cell_states: &mut [CellState],
                   rule: &Rule1D, inactive: &CellType, dt: &CellType,
                   width: usize, start: usize) -> TypeCounter {
        let mut count_map = TypeCounter::new();
        for local in 0..next_cells.len() {
            let new_type = *Self::next_type(cells, rule, inactive, width, start + local);
            next_cells[local] = new_type;
            cell_states[local].transition(&new_type);
            if new_type != *dt {
                count_map.add(new_type);
            }
        }
        count_map
    }
    fn recompute_counts_from_cells(&mut self, new_counts: &TypeCounter) {
        self.counts_current.clear();
        let mut total_count: u64 = self.cells.len() as u64;
        let mut new_dominant_type: (&CellType, u64) = (&CellType::inactive(), 0);
        for (k, v) in new_counts.iter() {
            self.counts_current.insert(k.0, *v);
            total_count -= *v;
            // check if peak count needs to be updated
            self.peak_counts.entry(k.0).and_modify(|peak| {
                if *v > *peak {
                    *peak = *v;
                }
            }).or_insert(*v);
            if *v >= new_dominant_type.1 {
                new_dominant_type = (k, *v);
            }
        }
        // add the dominant type
        self.counts_current.insert(self.dominant_type.0, total_count);
        self.peak_counts.entry(self.dominant_type.0).and_modify(|peak| {
            if total_count > *peak {
                *peak = total_count;
            }
        }).or_insert(total_count);
        if total_count < new_dominant_type.1 {
            self.dominant_type = *new_dominant_type.0;
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
        let width = self.width;
        // Disjoint field borrows shared by both paths; threads can't borrow `&self`.
        let cells = &self.cells;
        let next_cells = &mut self.next_cells;
        let cell_states = &mut self.cell_states;
        let rule = &self.rule;
        let inactive = &self.inactive;
        let dt = &self.dominant_type;

        let count_map = if threads <= 1 || width < 8192 {
            Self::step_chunk(cells, next_cells, cell_states, rule, inactive, dt, width, 0)
        } else {
            // One thread per chunk; `chunks_mut` hands each thread a disjoint
            // mutable slice, so the writes are sound without locking.
            let chunk = (width + threads - 1) / threads;
            std::thread::scope(|s| {
                let handles: Vec<_> = (0..)
                    .map(|t| t * chunk)
                    .zip(next_cells.chunks_mut(chunk))
                    .zip(cell_states.chunks_mut(chunk))
                    .map(|((start, next_chunk), state_chunk)| {
                        s.spawn(move || {
                            Self::step_chunk(cells, next_chunk, state_chunk, rule, inactive, dt, width, start)
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

        // Swap the buffers: cells becomes the freshly-computed next_cells,
        // and next_cells becomes the old buffer ready to be overwritten next step.
        std::mem::swap(&mut self.cells, &mut self.next_cells);

        // Update counts and peaks
        Self::recompute_counts_from_cells(self, &count_map);
        self.step = self.step.saturating_add(1);
    }
}
