//! 1D grid implementation.

use std::io::Error;
use lasso2::Spur;
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize};
use crate::types::{CellState, CellType};
use crate::rules::{Rule1D, TypeCounter};
use crate::threads::thread_count;

/// 1D grid containing cells and a 1D rule.
///
/// Create with [`Grid1D::new`], then call [`Grid1D::step`] repeatedly.
/// Data stored as struct-of-arrays for cache-friendly stepping.
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
    /// Age (steps in current state) per cell.
    #[serde(skip)] pub(crate) ages: Vec<u32>,
    /// Current cell types.
    #[serde(skip)] pub(crate) cells: Vec<CellType>,
    /// Next-step type buffer (double buffer).
    #[serde(skip)] pub(crate) next_cells: Vec<CellType>,
    /// Flat circular-buffer history: cell i occupies [i*history_limit .. (i+1)*history_limit).
    #[serde(skip)] pub(crate) history_data: Vec<CellType>,
    /// Write head index (0..history_limit) for each cell's circular history buffer.
    #[serde(skip)] pub(crate) history_heads: Vec<u8>,
    /// Number of entries currently stored in each cell's circular history buffer.
    #[serde(skip)] pub(crate) history_counts: Vec<u8>,
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
    /// Type with the highest count (dominant); skipped during counting.
    #[serde(skip)] pub(crate) dominant_type: CellType,
}

impl<'de> Deserialize<'de> for Grid1D {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
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
        let cells: Vec<CellType> = im.cell_states.iter().map(|cs| cs.current).collect();
        let next_cells: Vec<CellType> = vec![im.inactive; im.width];
        let ages: Vec<u32> = im.cell_states.iter().map(|cs| cs.age_in_state).collect();
        let history_data = Self::soa_history(&im.cell_states, im.history_limit);
        let history_heads = Self::soa_heads(&im.cell_states, im.history_limit);
        let history_counts = Self::soa_counts(&im.cell_states, im.history_limit);
        let dominant_type: CellType = im.counts_current.iter()
            .max_by_key(|entry| entry.1)
            .map(|(spur, _)| CellType(*spur))
            .unwrap_or_else(|| im.inactive.clone());
        Ok(Grid1D {
            width: im.width,
            history_limit: im.history_limit,
            ages,
            cells,
            next_cells,
            history_data,
            history_heads,
            history_counts,
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
    /// Convert deserialized CellState vectors to SoA history arrays.
    pub(crate) fn soa_history(states: &[CellState], limit: usize) -> Vec<CellType> {
        if limit == 0 { return Vec::new(); }
        let mut data = vec![CellType::inactive(); states.len() * limit];
        for (i, cs) in states.iter().enumerate() {
            let base = i * limit;
            for (j, ct) in cs.history.iter().enumerate() {
                data[base + j] = *ct;
            }
        }
        data
    }
    pub(crate) fn soa_heads(states: &[CellState], limit: usize) -> Vec<u8> {
        if limit == 0 { return Vec::new(); }
        states.iter().map(|cs| (cs.history.len() % limit) as u8).collect()
    }
    pub(crate) fn soa_counts(states: &[CellState], limit: usize) -> Vec<u8> {
        if limit == 0 { return Vec::new(); }
        states.iter().map(|cs| cs.history.len() as u8).collect()
    }

    /// Transition cell `idx` to `new_type` in SoA format.
    #[inline]
    fn transition_cell(&mut self, idx: usize, new_type: CellType) {
        let cur = self.cells[idx];
        let hl = self.history_limit;
        if hl > 0 {
            let base = idx * hl;
            let h = self.history_heads[idx] as usize;
            self.history_data[base + h] = cur;
            self.history_heads[idx] = ((h + 1) % hl) as u8;
            let c = self.history_counts[idx] as usize;
            if c < hl {
                self.history_counts[idx] = (c + 1) as u8;
            }
        }
        if cur == new_type {
            self.ages[idx] = self.ages[idx].saturating_add(1);
        } else {
            self.ages[idx] = 0;
        }
        self.cells[idx] = new_type;
    }

    /// Construct a new 1D grid.
    ///
    /// `initial.len()` must equal `width`.
    pub fn new(width: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule1D) -> Self {
        assert_eq!(initial.len(), width, "initial types len must equal width");
        assert!(history_limit <= 255, "history_limit must be <= 255 for SoA layout");
        let next_cells: Vec<CellType> = vec![CellType::inactive(); width];
        let ages: Vec<u32> = vec![0; width];
        let (history_data, history_heads, history_counts) = if history_limit == 0 {
            (Vec::new(), Vec::new(), Vec::new())
        } else {
            let data = vec![CellType::inactive(); width * history_limit];
            let heads = vec![0u8; width];
            let counts = vec![0u8; width];
            (data, heads, counts)
        };
        let mut counts_current: std::collections::HashMap<Spur, u64> = std::collections::HashMap::new();
        for c in &initial { *counts_current.entry(c.0).or_insert(0) += 1; }
        let dominant_type = counts_current.iter()
            .max_by_key(|entry| entry.1)
            .map(|(spur, _)| CellType(*spur))
            .unwrap_or_else(|| CellType::inactive());
        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self { width, history_limit, ages, cells: initial, next_cells,
            history_data, history_heads, history_counts, step: 0, rule,
            counts_current, peak_counts, inactive, dominant_type }
    }

    /// Transitions the given cell to `new_type`.
    /// Used by interactive or programmatic routines that change grid
    /// state outside of stepping (i.e. grid painting).
    pub fn transition_state_and_buffer(&mut self, idx: usize, new_type: &CellType) -> Option<Error> {
        if idx > self.width {
            Some(Error::new(std::io::ErrorKind::InvalidInput, "Index out of bounds"))
        } else {
            self.transition_cell(idx, *new_type);
            None
        }
    }

    /// Type at `idx`, or `inactive` when out of bounds.
    #[inline]
    fn get_type_or_inactive(cells: &[CellType], inactive: CellType, width: usize, idx: isize) -> CellType {
        if idx < 0 || idx as usize >= width { inactive } else { cells[idx as usize] }
    }

    /// Evaluate subrules in order for the cell at `idx`, returning its next type.
    #[inline]
    fn next_type(cells: &[CellType], rule: &Rule1D, inactive: &CellType,
                  width: usize, idx: usize) -> CellType {
        let current_type = &cells[idx];
        for s in &rule.subrules {
            if current_type != &s.current_type { continue; }
            let n = s.n as isize;
            let inactive = *inactive;
            let out = match n {
                1 => {
                    let w = [
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize - 1),
                        current_type.clone(),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize + 1),
                    ];
                    if s.applies(&w) { s.output_type.clone() } else { continue }
                }
                2 => {
                    let w = [
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize - 2),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize - 1),
                        current_type.clone(),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize + 1),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize + 2),
                    ];
                    if s.applies(&w) { s.output_type.clone() } else { continue }
                }
                3 => {
                    let w = [
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize - 3),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize - 2),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize - 1),
                        current_type.clone(),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize + 1),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize + 2),
                        Self::get_type_or_inactive(cells, inactive, width, idx as isize + 3),
                    ];
                    if s.applies(&w) { s.output_type.clone() } else { continue }
                }
                _ => continue,
            };
            if let Some(r) = s.randomness {
                let mut rng = rand::thread_rng();
                if rng.r#gen::<f64>() < r { continue; }
            }
            return out;
        }
        *inactive
    }

    /// Compute the next state for a chunk, writing into disjoint output slices.
    fn step_chunk(
        cells: &[CellType], next_cells: &mut [CellType],
        ages: &mut [u32],
        history_data: &mut [CellType], history_heads: &mut [u8], history_counts: &mut [u8],
        history_limit: usize,
        rule: &Rule1D, inactive: CellType, dt: CellType,
        width: usize, start: usize,
    ) -> TypeCounter {
        let mut count_map = TypeCounter::new();
        for local in 0..next_cells.len() {
            let new_type = Self::next_type(cells, rule, &inactive, width, start + local);
            next_cells[local] = new_type;
            let cur = cells[start + local];
            if history_limit > 0 {
                let base_hist = local * history_limit;
                let h = history_heads[local] as usize;
                history_data[base_hist + h] = cur;
                history_heads[local] = ((h + 1) % history_limit) as u8;
                let c = history_counts[local] as usize;
                if c < history_limit {
                    history_counts[local] = (c + 1) as u8;
                }
            }
            if cur == new_type {
                ages[local] = ages[local].saturating_add(1);
            } else {
                ages[local] = 0;
            }
            if new_type != dt {
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
            self.peak_counts.entry(k.0).and_modify(|peak| {
                if *v > *peak { *peak = *v; }
            }).or_insert(*v);
            if *v >= new_dominant_type.1 {
                new_dominant_type = (k, *v);
            }
        }
        self.counts_current.insert(self.dominant_type.0, total_count);
        self.peak_counts.entry(self.dominant_type.0).and_modify(|peak| {
            if total_count > *peak { *peak = total_count; }
        }).or_insert(total_count);
        if total_count < new_dominant_type.1 {
            self.dominant_type = *new_dominant_type.0;
        }
    }

    /// Advance the automaton by one step using double-buffering.
    pub fn step(&mut self) {
        let threads = thread_count();
        let width = self.width;
        let hl = self.history_limit;
        let cells = &self.cells;
        let next_cells = &mut self.next_cells;
        let ages = &mut self.ages;
        let history_data = &mut self.history_data;
        let history_heads = &mut self.history_heads;
        let history_counts = &mut self.history_counts;
        let rule = &self.rule;
        let inactive = self.inactive;
        let dt = self.dominant_type;

        let count_map = if threads <= 1 || width < 8192 || hl == 0 {
            Self::step_chunk(cells, next_cells, ages,
                history_data, history_heads, history_counts, hl,
                rule, inactive, dt, width, 0)
        } else {
            let chunk = (width + threads - 1) / threads;
            std::thread::scope(|s| {
                let handles: Vec<_> = (0..)
                    .map(|t| t * chunk)
                    .zip(next_cells.chunks_mut(chunk))
                    .zip(ages.chunks_mut(chunk))
                    .zip(history_data.chunks_mut(chunk * hl))
                    .zip(history_heads.chunks_mut(chunk))
                    .zip(history_counts.chunks_mut(chunk))
                    .map(|(((((start, next_chunk), age_chunk), hist_data_chunk), head_chunk), count_chunk)| {
                        s.spawn(move || {
                            Self::step_chunk(cells, next_chunk, age_chunk,
                                hist_data_chunk, head_chunk, count_chunk, hl,
                                rule, inactive, dt, width, start)
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

        std::mem::swap(&mut self.cells, &mut self.next_cells);
        Self::recompute_counts_from_cells(self, &count_map);
        self.step = self.step.saturating_add(1);
    }

    /// Current type of cell at `idx`.
    #[inline]
    pub fn cell_type(&self, idx: usize) -> CellType {
        self.cells.get(idx).copied().unwrap_or(self.inactive)
    }

    /// Age of cell at `idx` (steps in current state).
    #[inline]
    pub fn cell_age(&self, idx: usize) -> u32 {
        self.ages.get(idx).copied().unwrap_or(0)
    }

    /// History entries for cell `idx` in FIFO order (oldest first).
    pub fn cell_history(&self, idx: usize) -> Vec<CellType> {
        if self.history_limit == 0 { return Vec::new(); }
        let base = idx * self.history_limit;
        let head = self.history_heads[idx] as usize;
        let count = self.history_counts[idx] as usize;
        let mut hist = Vec::with_capacity(count);
        for i in 0..count {
            let pos = if count == self.history_limit {
                (head + i) % self.history_limit
            } else {
                (head - count + i) % self.history_limit
            };
            hist.push(self.history_data[base + pos]);
        }
        hist
    }

    /// Reconstruct the old-style CellState vectors (for serialization / compatibility).
    pub fn to_cell_states(&self) -> Vec<CellState> {
        self.cells.iter().enumerate().map(|(i, ct)| {
            CellState {
                history: self.cell_history(i).into(),
                age_in_state: self.ages[i],
                history_limit: self.history_limit,
                current: *ct,
            }
        }).collect()
    }
}
