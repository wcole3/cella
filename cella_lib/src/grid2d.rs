//! 2D grid implementation.

use std::io::Error;
use crate::rules::{Rule2D, TypeCounter};
use crate::threads::thread_count;
use crate::types::{CellState, CellType};
use lasso2::Spur;
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize};

/// 2D grid containing cells and a 2D rule.
///
/// Create with [`Grid2D::new`], then call [`Grid2D::step`] repeatedly.
/// Cells are stored row-major; data organized as struct-of-arrays.
///
/// Example
/// ```rust
/// use cella_lib::{Grid2D, Rule2D, Rule2DSubrule, Neighborhood2D, CellType, CountOp};
/// let alive = CellType::from("Alive");
/// let inactive = CellType::inactive();
/// let rule = Rule2D { subrules: vec![
///   Rule2DSubrule::new(alive.clone(), alive.clone(), 4, CountOp::Gt, 1, Neighborhood2D::Moore, inactive.clone(), None, None),
///   Rule2DSubrule::new(alive.clone(), alive.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, alive.clone(), None, None),
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
    /// Age (steps in current state) per cell.
    #[serde(skip)] pub(crate) ages: Vec<u32>,
    /// Current cell types (row-major).
    #[serde(skip)] pub(crate) cells: Vec<CellType>,
    /// Next-step type buffer (double buffer).
    #[serde(skip)] pub(crate) next_cells: Vec<CellType>,
    /// Flat circular-buffer history: cell i occupies [i*history_limit .. (i+1)*history_limit).
    #[serde(skip)] pub(crate) history_data: Vec<CellType>,
    /// Write head for each cell's circular history buffer.
    #[serde(skip)] pub(crate) history_heads: Vec<u8>,
    /// Entry count for each cell's circular history buffer.
    #[serde(skip)] pub(crate) history_counts: Vec<u8>,
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
    /// Type with the highest count (dominant); skipped during counting.
    #[serde(skip)] pub(crate) dominant_type: CellType,
}

impl<'de> Deserialize<'de> for Grid2D {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
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
        let cells: Vec<CellType> = intermediate.cell_states.iter().map(|cs| cs.current).collect();
        let next_cells: Vec<CellType> = vec![intermediate.inactive.clone(); intermediate.width * intermediate.height];
        let ages: Vec<u32> = intermediate.cell_states.iter().map(|cs| cs.age_in_state).collect();
        let history_data = Grid1D::soa_history(&intermediate.cell_states, intermediate.history_limit);
        let history_heads = Grid1D::soa_heads(&intermediate.cell_states, intermediate.history_limit);
        let history_counts = Grid1D::soa_counts(&intermediate.cell_states, intermediate.history_limit);
        let dominant_type: CellType = intermediate.counts_current.iter()
            .max_by_key(|entry| entry.1)
            .map(|(spur, _)| CellType(*spur))
            .unwrap_or_else(|| intermediate.inactive.clone());
        Ok(Grid2D {
            width: intermediate.width,
            height: intermediate.height,
            history_limit: intermediate.history_limit,
            ages,
            cells,
            next_cells,
            history_data,
            history_heads,
            history_counts,
            step: intermediate.step,
            rule: intermediate.rule,
            counts_current: intermediate.counts_current,
            peak_counts: intermediate.peak_counts,
            inactive: intermediate.inactive,
            dominant_type,
        })
    }
}

use crate::grid1d::Grid1D;

impl std::fmt::Debug for Grid2D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grid2D")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("history_limit", &self.history_limit)
            .field("step", &self.step)
            .field("rule", &self.rule)
            .field("counts_current", &self.counts_current)
            .field("peak_counts", &self.peak_counts)
            .finish()
    }
}

impl Grid2D {
    /// Construct a new 2D grid.
    pub fn new(width: usize, height: usize, history_limit: usize, initial: Vec<CellType>, rule: Rule2D) -> Self {
        assert_eq!(initial.len(), width * height, "initial types len must equal width*height");
        assert!(history_limit <= 255, "history_limit must be <= 255 for SoA layout");
        let total = width * height;
        let next_cells: Vec<CellType> = vec![CellType::inactive(); total];
        let ages: Vec<u32> = vec![0; total];
        let (history_data, history_heads, history_counts) = if history_limit == 0 {
            (Vec::new(), Vec::new(), Vec::new())
        } else {
            let data = vec![CellType::inactive(); total * history_limit];
            let heads = vec![0u8; total];
            let counts = vec![0u8; total];
            (data, heads, counts)
        };
        let mut counts_current: std::collections::HashMap<Spur, u64> = std::collections::HashMap::new();
        let mut dominant_type: (CellType, u64) = (CellType::inactive(), 0);
        for c in &initial {
            let cnt = *counts_current.entry(c.0).or_insert(0) + 1;
            *counts_current.entry(c.0).or_insert(cnt) = cnt;
            if cnt > dominant_type.1 {
                dominant_type = (c.clone(), cnt);
            }
        }
        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self { width, height, history_limit, ages, cells: initial,
            next_cells, history_data, history_heads, history_counts,
            step: 0, rule, counts_current, peak_counts, inactive, dominant_type: dominant_type.0 }
    }

    /// Transition cell `idx` to `new_type`.
    pub fn transition_state_and_buffer(&mut self, idx: usize, new_type: &CellType) -> Option<Error> {
        if idx > self.width * self.height {
            Some(Error::new(std::io::ErrorKind::InvalidInput, "Index out of bounds"))
        } else {
            self.transition_cell(idx, *new_type);
            None
        }
    }

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

    fn recompute_counts_from_cells(&mut self, new_counts: &TypeCounter) {
        self.counts_current.clear();
        let mut total_count: u64 = self.cells.len() as u64;
        let mut new_dominant_type: (&CellType, u64) = (&CellType::inactive(), 0);
        for (k, v) in new_counts.iter() {
            self.counts_current.entry(k.0).or_insert(*v);
            total_count -= *v;
            self.peak_counts.entry(k.0).and_modify(|count| {
                if *count < *v { *count = *v; }
            }).or_insert(*v);
            if *v >= new_dominant_type.1 {
                new_dominant_type.0 = k
            }
        }
        self.counts_current.entry(self.dominant_type.0).or_insert(total_count);
        self.peak_counts.entry(self.dominant_type.0).and_modify(|count| {
                if *count < total_count { *count = total_count; }
            }).or_insert(total_count);
        if total_count < new_dominant_type.1 {
            self.dominant_type = *new_dominant_type.0;
        }
    }
    
    // TODO need tests to cover
    /// Type at `(x, y)`, or `inactive` when out of bounds.
    #[inline]
    fn neighbor(cells: &[CellType], inactive: CellType, width: usize, height: usize, x: isize, y: isize) -> CellType {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
            inactive
        } else {
            cells[y as usize * width + x as usize]
        }
    }

    /// Evaluate subrules in order for the cell at `(x, y)`.
    /// Neighbor counting inlined — no closure dispatch.
    #[inline]
    fn next_type(cells: &[CellType], rule: &Rule2D, inactive: CellType,
                  width: usize, height: usize, x: isize, y: isize) -> CellType {
        let idx = y as usize * width + x as usize;
        let current_type = cells[idx];
        for sr in &rule.subrules {
            if current_type != sr.current_type { continue; }
            let crit = sr.criteria_type;
            let mut neighbors = 0u32;
            for off in &sr.offsets {
                if Self::neighbor(cells, inactive, width, height, x + off.0 as isize, y + off.1 as isize) == crit {
                    neighbors += 1;
                }
                if sr.op == crate::rules::CountOp::Gt && sr.limit.is_none() && neighbors >= sr.count {
                    break;
                }
            }
            if sr.eval_condition(neighbors) {
                if let Some(r) = sr.randomness {
                    let mut rng = rand::thread_rng();
                    if rng.r#gen::<f64>() < r { continue; }
                }
                return sr.output_type.clone();
            }
        }
        inactive
    }

    /// Compute the next state for a chunk.
    fn step_chunk(
        cells: &[CellType], next_cells: &mut [CellType],
        ages: &mut [u32],
        history_data: &mut [CellType], history_heads: &mut [u8], history_counts: &mut [u8],
        history_limit: usize,
        rule: &Rule2D, inactive: CellType, dt: CellType,
        width: usize, height: usize, start: usize,
    ) -> TypeCounter {
        let mut count_map = TypeCounter::new();
        let mut x = (start % width) as isize;
        let mut y = (start / width) as isize;
        for local in 0..next_cells.len() {
            let new_type = Self::next_type(cells, rule, inactive, width, height, x, y);
            let cur = cells[y as usize * width + x as usize];
            next_cells[local] = new_type;
            if history_limit > 0 {
                let base = local * history_limit;
                let h = history_heads[local] as usize;
                history_data[base + h] = cur;
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
            x += 1;
            if x == width as isize {
                x = 0;
                y += 1;
            }
        }
        count_map
    }

    /// Advance the automaton by one step using double-buffering.
    pub fn step(&mut self) {
        let threads = thread_count();
        let width = self.width;
        let height = self.height;
        let total = width * height;
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

        let count_map = if threads <= 1 || total < 4096 || hl == 0 {
            Self::step_chunk(cells, next_cells, ages,
                history_data, history_heads, history_counts, hl,
                rule, inactive, dt, width, height, 0)
        } else {
            let chunk = (total + threads - 1) / threads;
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
                                rule, inactive, dt, width, height, start)
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

    /// Age of cell at `idx`.
    #[inline]
    pub fn cell_age(&self, idx: usize) -> u32 {
        self.ages.get(idx).copied().unwrap_or(0)
    }

    /// History entries for cell `idx` in FIFO order.
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

    /// Reconstruct old-style CellState vectors.
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
