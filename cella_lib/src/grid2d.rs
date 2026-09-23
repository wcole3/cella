//! 2D grid implementation.

use crate::chunking::{OutChunk, split_chunks};
use crate::resize::{ResizeError, checked_cells, recount, remap_blocks};
use crate::rules::{Rule2D, Rule2DPlan, TypeCounter, apply_counts};
use crate::threads::{chunks_for_work, pool};
use crate::types::{CellState, CellType};
use lasso2::Spur;
use crate::rng::{STREAM_RULE, cell_rand};
use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};
use std::io::Error;

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
    #[serde(skip)]
    pub(crate) ages: Vec<u32>,
    /// Current cell types (row-major).
    #[serde(skip)]
    pub(crate) cells: Vec<CellType>,
    /// Next-step type buffer (double buffer).
    #[serde(skip)]
    pub(crate) next_cells: Vec<CellType>,
    /// Flat circular-buffer history: cell i occupies [i*history_limit .. (i+1)*history_limit).
    #[serde(skip)]
    pub(crate) history_data: Vec<CellType>,
    /// Write head for each cell's circular history buffer.
    /// TODO need to check if there is still a need to maintain this; every cell's history gets
    /// updates when the buffers swap, so presumably we could maintain a single head
    /// and update when the buffers swap. That would save the array memory
    #[serde(skip)]
    pub(crate) history_heads: Vec<u8>,
    /// Entry count for each cell's circular history buffer.
    #[serde(skip)]
    pub(crate) history_counts: Vec<u8>,
    /// Current simulation step.
    pub step: u64,
    /// Seed for every `randomness` draw the subrules make. Two grids with the
    /// same seed, rule and cells step identically on any thread count; the
    /// draw is `cell_rand(seed, step, cell, STREAM_RULE + subrule)`.
    #[serde(default)]
    pub seed: u64,
    /// Rule used for updates.
    pub rule: Rule2D,
    /// Current count of cells per type name.
    pub counts_current: std::collections::HashMap<Spur, u64>,
    /// Peak (max-so-far) count of cells per type name since start/reset.
    pub peak_counts: std::collections::HashMap<Spur, u64>,
    /// Reference to inactive cell type.
    pub inactive: CellType,
    /// Type with the highest count (dominant); skipped during counting.
    #[serde(skip)]
    pub(crate) dominant_type: CellType,
    /// Optional external transition model; when present it replaces the
    /// subrule engine for stepping. See [`crate::external::ExternalModel`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Box<dyn crate::external::ExternalModel>>,
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
            #[serde(default)]
            seed: u64,
            rule: Rule2D,
            counts_current: std::collections::HashMap<Spur, u64>,
            peak_counts: std::collections::HashMap<Spur, u64>,
            inactive: CellType,
            #[serde(default)]
            model: Option<Box<dyn crate::external::ExternalModel>>,
        }
        let intermediate = Grid2DIntermediate::deserialize(d)?;
        let cells: Vec<CellType> = intermediate
            .cell_states
            .iter()
            .map(|cs| cs.current)
            .collect();
        let next_cells: Vec<CellType> =
            vec![intermediate.inactive; intermediate.width * intermediate.height];
        let ages: Vec<u32> = intermediate
            .cell_states
            .iter()
            .map(|cs| cs.age_in_state)
            .collect();
        let history_data = soa_history(&intermediate.cell_states, intermediate.history_limit);
        let history_heads = soa_heads(&intermediate.cell_states, intermediate.history_limit);
        let history_counts = soa_counts(&intermediate.cell_states, intermediate.history_limit);
        let dominant_type: CellType = intermediate
            .counts_current
            .iter()
            .max_by_key(|entry| entry.1)
            .map(|(spur, _)| CellType(*spur))
            .unwrap_or_else(|| intermediate.inactive);
        let mut grid = Grid2D {
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
            seed: intermediate.seed,
            rule: intermediate.rule,
            counts_current: intermediate.counts_current,
            peak_counts: intermediate.peak_counts,
            inactive: intermediate.inactive,
            dominant_type,
            model: None,
        };
        if let Some(model) = intermediate.model {
            grid.attach_model(model).map_err(serde::de::Error::custom)?;
        }
        Ok(grid)
    }
}

use crate::state::{soa_counts, soa_heads, soa_history};

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
    pub fn new(
        width: usize,
        height: usize,
        history_limit: usize,
        initial: Vec<CellType>,
        rule: Rule2D,
    ) -> Self {
        assert_eq!(
            initial.len(),
            width * height,
            "initial types len must equal width*height"
        );
        assert!(
            history_limit <= 255,
            "history_limit must be <= 255 for SoA layout"
        );
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
        let mut counts_current: std::collections::HashMap<Spur, u64> =
            std::collections::HashMap::new();
        let mut dominant_type: (CellType, u64) = (CellType::inactive(), 0);
        for c in &initial {
            let cnt = counts_current.entry(c.0).or_insert(0);
            *cnt += 1;
            if *cnt > dominant_type.1 {
                dominant_type = (*c, *cnt);
            }
        }
        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self {
            width,
            height,
            history_limit,
            ages,
            cells: initial,
            next_cells,
            history_data,
            history_heads,
            history_counts,
            step: 0,
            seed: 0,
            rule,
            counts_current,
            peak_counts,
            inactive,
            dominant_type: dominant_type.0,
            model: None,
        }
    }

    /// Same grid, different seed for the subrules' `randomness` draws.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.set_seed(seed);
        self
    }

    /// Set the seed for `randomness` draws and hand the same seed to the
    /// attached model (see [`crate::external::ExternalModel::set_seed`]), so
    /// one number reseeds the whole simulation.
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        if let Some(m) = self.model.as_deref_mut() {
            m.set_seed(seed);
        }
    }

    /// All current cell types, row-major. Read-only; paint through
    /// [`Self::transition_state_and_buffer`].
    #[inline]
    pub fn cells(&self) -> &[CellType] {
        &self.cells
    }

    /// Replace every cell and start over: ages become 0, per-cell history is
    /// cleared, `step` returns to 0, the population counts are recomputed and
    /// an attached model is re-attached against the new cells.
    ///
    /// The model is validated against the new cells *before* anything
    /// changes, so an error leaves the grid exactly as it was. `cells.len()`
    /// must equal `width * height`.
    pub fn reset_cells(&mut self, cells: Vec<CellType>) -> Result<(), crate::external::ModelError> {
        let total = self.width * self.height;
        if cells.len() != total {
            return Err(crate::external::ModelError::LayerLength {
                layer: "cells",
                expected: total,
                got: cells.len(),
            });
        }
        let model = match &self.model {
            Some(m) => {
                let mut fresh = m.boxed_clone();
                fresh.attach(&crate::external::GridView {
                    width: self.width,
                    height: self.height,
                    cells: &cells,
                    inactive: self.inactive,
                })?;
                Some(fresh)
            }
            None => None,
        };
        let mut counts_current: std::collections::HashMap<Spur, u64> =
            std::collections::HashMap::new();
        let mut dominant: (CellType, u64) = (self.inactive, 0);
        for c in &cells {
            let cnt = counts_current.entry(c.0).or_insert(0);
            *cnt += 1;
            if *cnt > dominant.1 {
                dominant = (*c, *cnt);
            }
        }
        self.cells = cells;
        self.next_cells = vec![self.inactive; total];
        self.ages = vec![0; total];
        if self.history_limit > 0 {
            self.history_data = vec![self.inactive; total * self.history_limit];
            self.history_heads = vec![0u8; total];
            self.history_counts = vec![0u8; total];
        }
        self.step = 0;
        self.peak_counts = counts_current.clone();
        self.counts_current = counts_current;
        self.dominant_type = dominant.0;
        self.model = model;
        Ok(())
    }

    /// Change the grid to `width` x `height` mid-run, anchored top-left.
    ///
    /// Every cell in the overlap keeps its type, age and history. New cells
    /// are Inactive, with age 0 and no history. `step`, `seed`, `rule` and
    /// `peak_counts` carry on; the current counts are recomputed. An attached
    /// model re-fits itself through [`crate::external::ExternalModel::resize`].
    ///
    /// All or nothing: the model is resized on a clone first, and on any
    /// error the grid is left exactly as it was. Resizing to the current size
    /// is a no-op.
    ///
    /// Randomness is keyed by the flat index `y * width + x`, so a *width*
    /// change gives surviving cells below the first row new random streams
    /// from now on (see "Resizing a grid" in docs/lib.md).
    pub fn resize(&mut self, width: usize, height: usize) -> Result<(), ResizeError> {
        let total = checked_cells(width, height, self.history_limit)?;
        if (width, height) == (self.width, self.height) {
            return Ok(());
        }
        let old = (self.width, self.height);
        let new = (width, height);
        let cells = remap_blocks(&self.cells, 1, old, new, self.inactive);
        let model = match &self.model {
            Some(m) => {
                let mut fresh = m.boxed_clone();
                fresh.resize(
                    old,
                    &crate::external::GridView {
                        width,
                        height,
                        cells: &cells,
                        inactive: self.inactive,
                    },
                )?;
                Some(fresh)
            }
            None => None,
        };
        let hl = self.history_limit;
        self.ages = remap_blocks(&self.ages, 1, old, new, 0);
        if hl > 0 {
            self.history_data = remap_blocks(&self.history_data, hl, old, new, self.inactive);
            self.history_heads = remap_blocks(&self.history_heads, 1, old, new, 0);
            self.history_counts = remap_blocks(&self.history_counts, 1, old, new, 0);
        }
        self.next_cells = vec![self.inactive; total];
        self.cells = cells;
        self.width = width;
        self.height = height;
        self.model = model;
        let (counts, dominant) = recount(&self.cells, self.inactive, &mut self.peak_counts);
        self.counts_current = counts;
        self.dominant_type = dominant;
        Ok(())
    }

    /// Transition cell `idx` to `new_type`.
    pub fn transition_state_and_buffer(
        &mut self,
        idx: usize,
        new_type: &CellType,
    ) -> Option<Error> {
        if idx >= self.width * self.height {
            Some(Error::new(
                std::io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ))
        } else {
            self.transition_cell(idx, *new_type);
            if let Some(m) = self.model.as_deref_mut() {
                m.on_paint(idx, *new_type);
            }
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
            // `% hl` would be a hardware divide (hl is a runtime value) on every
            // cell of every step; h is always < hl, so a compare suffices.
            self.history_heads[idx] = if h + 1 == hl { 0 } else { (h + 1) as u8 };
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

    /// Type at `(x, y)`, or `inactive` when out of bounds.
    /// Only used for cells within `plan.pad` of a border; see `next_type_interior`.
    #[inline]
    fn neighbor(
        cells: &[CellType],
        inactive: CellType,
        width: usize,
        height: usize,
        x: isize,
        y: isize,
    ) -> CellType {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
            inactive
        } else {
            cells[y as usize * width + x as usize]
        }
    }

    /// Evaluate subrules for an *interior* cell — one at least `plan.pad` cells from
    /// every border, so every neighbor is guaranteed in bounds.
    ///
    /// Neighbors are reached by adding a precomputed linear offset to `idx`: no
    /// per-neighbor bounds comparisons and no `y * width + x` multiply.
    #[inline]
    fn next_type_interior(
        cells: &[CellType],
        rule: &Rule2D,
        plan: &Rule2DPlan,
        inactive: CellType,
        idx: usize,
        seed: u64,
        step: u64,
    ) -> CellType {
        let current_type = cells[idx];
        for (i, sr) in rule.subrules.iter().enumerate() {
            if current_type != sr.current_type {
                continue;
            }
            let offsets = plan.lin(i);
            let crit = sr.criteria_type;
            let early = sr.early_exit;
            let target = sr.count;
            let mut neighbors = 0u32;
            // A `Gt 0` subrule is satisfied with zero neighbors — skip the scan
            // entirely. (Restructuring the loop itself was tried and regressed
            // the Moore benches; see performance.md §8.)
            if !(early && target == 0) {
                for &off in offsets {
                    if cells[idx.wrapping_add_signed(off)] == crit {
                        neighbors += 1;
                    }
                    if early && neighbors >= target {
                        break;
                    }
                }
            }
            if sr.eval_condition(neighbors) {
                if let Some(r) = sr.randomness
                    && f64::from(cell_rand(seed, step, idx as u64, STREAM_RULE + i as u64)) < r
                {
                    continue;
                }
                return sr.output_type;
            }
        }
        inactive
    }

    /// Evaluate subrules for a cell near a border, treating out-of-bounds
    /// neighbors as `inactive`.
    ///
    // Every argument is a distinct piece of per-step state the hot loop
    // already has in hand; bundling them into a struct would just move the
    // field list to a constructor call at every call site.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn next_type_edge(
        cells: &[CellType],
        rule: &Rule2D,
        inactive: CellType,
        width: usize,
        height: usize,
        x: isize,
        y: isize,
        idx: usize,
        seed: u64,
        step: u64,
    ) -> CellType {
        let current_type = cells[idx];
        for (i, sr) in rule.subrules.iter().enumerate() {
            if current_type != sr.current_type {
                continue;
            }
            let crit = sr.criteria_type;
            let early = sr.early_exit;
            let target = sr.count;
            let mut neighbors = 0u32;
            if !(early && target == 0) {
                for off in &sr.offsets {
                    if Self::neighbor(
                        cells,
                        inactive,
                        width,
                        height,
                        x + off.0 as isize,
                        y + off.1 as isize,
                    ) == crit
                    {
                        neighbors += 1;
                    }
                    if early && neighbors >= target {
                        break;
                    }
                }
            }
            if sr.eval_condition(neighbors) {
                if let Some(r) = sr.randomness
                    && f64::from(cell_rand(seed, step, idx as u64, STREAM_RULE + i as u64)) < r
                {
                    continue;
                }
                return sr.output_type;
            }
        }
        inactive
    }

    /// Compute the next state for one chunk of the grid.
    ///
    /// The `history_limit > 0` test stays *inside* the per-cell loop on
    /// purpose: splitting the loop into with/without-history monomorphized
    /// variants was measured at +30–46 % across every 2D bench (the doubled
    /// body blows the inliner budget for `next_type_interior`) — see
    /// performance.md §8 E5. The branch itself is perfectly predicted.
    // Every argument is a distinct piece of per-chunk state the caller
    // already has in hand; bundling them into a struct would just move the
    // field list to a constructor call at every call site.
    #[allow(clippy::too_many_arguments)]
    fn step_chunk(
        cells: &[CellType],
        out: &mut OutChunk<'_>,
        history_limit: usize,
        rule: &Rule2D,
        plan: &Rule2DPlan,
        inactive: CellType,
        dt: CellType,
        width: usize,
        height: usize,
        seed: u64,
        step: u64,
    ) -> TypeCounter {
        let mut count_map = TypeCounter::new();
        let start = out.start;
        // Reborrow into locals: indexing through `&mut OutChunk` makes the loop
        // reload each slice's pointer and length from the struct on every access.
        let next_cells = &mut *out.next_cells;
        let ages = &mut *out.ages;
        let history_data = &mut *out.history_data;
        let history_heads = &mut *out.history_heads;
        let history_counts = &mut *out.history_counts;
        let pad = plan.pad;
        let len = next_cells.len();
        let mut x = start % width;
        let mut y = start / width;
        // A row is interior in y once it clears the top and bottom margins; the
        // per-cell test then only has to bound x.
        let mut row_interior = y >= pad && y + pad < height;
        let x_hi = width.saturating_sub(pad);
        for local in 0..len {
            let idx = start + local;
            let cur = cells[idx];
            let new_type = if row_interior && x >= pad && x < x_hi {
                Self::next_type_interior(cells, rule, plan, inactive, idx, seed, step)
            } else {
                Self::next_type_edge(
                    cells, rule, inactive, width, height, x as isize, y as isize, idx, seed, step,
                )
            };
            next_cells[local] = new_type;
            if history_limit > 0 {
                let base = local * history_limit;
                let h = history_heads[local] as usize;
                history_data[base + h] = cur;
                // `% history_limit` would be a hardware divide (the limit is a
                // runtime value) on every cell of every step; h is always
                // < history_limit, so a compare suffices.
                history_heads[local] = if h + 1 == history_limit {
                    0
                } else {
                    (h + 1) as u8
                };
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
            if x == width {
                x = 0;
                y += 1;
                row_interior = y >= pad && y + pad < height;
            }
        }
        count_map
    }

    /// The fast step for a [`crate::rules::PackedThreshold2D`] rule: the grid
    /// is stored one **bit** per cell (1 = `active`, 0 = inactive), each row
    /// packed into 64-bit integers, so one machine instruction processes 64
    /// cells at once.
    ///
    /// Returns `None` when the fast path can't be used — some cell is neither
    /// `active` nor inactive (e.g. the user painted a third type). The caller
    /// then runs the normal scalar path instead, so results are always
    /// identical either way; this check runs every step.
    ///
    /// # How one word is stepped
    ///
    /// **1. Line up the neighbors.** Every cell has up to 8 neighbors. By
    /// taking the word for the row above, the row itself, and the row below —
    /// each as-is, shifted one bit left, and shifted one bit right — we get
    /// eight words in which bit `j` holds one particular neighbor of cell
    /// `j`. (Bits falling off a word carry into the next word of the same
    /// row; the grid border and row ends shift in zeros, which matches the
    /// scalar rule "out of bounds counts as inactive".)
    ///
    /// **2. Count, in binary, 64 cells at once.** Adding eight 0-or-1 values
    /// gives a count from 0 to 8, which needs 4 binary digits. We keep those
    /// digits as four words `c0..c3`: bit `j` of `c0` is the 1s digit of cell
    /// `j`'s count, bit `j` of `c1` the 2s digit, and so on. Each neighbor
    /// word is added with the "carry" pattern below — the same idea as adding
    /// 1 to a binary number by hand, done for all 64 cells simultaneously:
    ///
    /// ```text
    /// carry = c0 & p;  c0 ^= p;   // add p to the 1s digit; overflow carries
    /// ...same for c1, c2, then c3
    /// ```
    ///
    /// **3. Look up the answer.** The rule was precomputed into
    /// `table[current][count]` (see `PackedThreshold2D`). For each count value
    /// the table cares about, build a mask of the cells whose count equals it
    /// (compare all four digit-words at once), AND it with "is the cell
    /// currently active" if the table distinguishes that, and OR everything
    /// together. The result word *is* the next generation of those 64 cells.
    ///
    /// The per-cell bookkeeping (writing next types, ages, history, counts)
    /// still runs as a normal loop afterwards; only the rule evaluation is
    /// bit-parallel.
    fn step_packed(&mut self, pt: crate::rules::PackedThreshold2D) -> Option<TypeCounter> {
        let (w, h) = (self.width, self.height);
        let active = pt.active;
        let inactive = self.inactive;
        if self.cells.iter().any(|&c| c != active && c != inactive) {
            return None;
        }
        let wpr = w.div_ceil(64); // words per row
        let mut bits = vec![0u64; wpr * h];
        for y in 0..h {
            for x in 0..w {
                if self.cells[y * w + x] == active {
                    bits[y * wpr + x / 64] |= 1 << (x % 64);
                }
            }
        }
        let zero_row = vec![0u64; wpr];
        let mut next_bits = vec![0u64; wpr * h];
        let tail_mask = if w % 64 != 0 {
            (1u64 << (w % 64)) - 1
        } else {
            !0u64
        };
        for y in 0..h {
            let cur_row = &bits[y * wpr..(y + 1) * wpr];
            let up: &[u64] = if y > 0 {
                &bits[(y - 1) * wpr..y * wpr]
            } else {
                &zero_row
            };
            let dn: &[u64] = if y + 1 < h {
                &bits[(y + 1) * wpr..(y + 2) * wpr]
            } else {
                &zero_row
            };
            for k in 0..wpr {
                let shl = |r: &[u64]| (r[k] << 1) | if k > 0 { r[k - 1] >> 63 } else { 0 };
                let shr = |r: &[u64]| (r[k] >> 1) | if k + 1 < wpr { r[k + 1] << 63 } else { 0 };
                // In neighborhood_offsets(Moore, 1) order — sorted (dx, dy):
                // (-1,-1) (-1,0) (-1,1) (0,-1) (0,1) (1,-1) (1,0) (1,1).
                // dx = -1 means "west neighbor": bit j reads bit j-1 => shl.
                let planes = [
                    shl(up),
                    shl(cur_row),
                    shl(dn),
                    up[k],
                    dn[k],
                    shr(up),
                    shr(cur_row),
                    shr(dn),
                ];
                // Sum the selected 1-bit planes into count bit-planes c3..c0.
                let (mut c0, mut c1, mut c2, mut c3) = (0u64, 0u64, 0u64, 0u64);
                for (j, &p) in planes.iter().enumerate() {
                    if !pt.slots[j] {
                        continue;
                    }
                    let carry0 = c0 & p;
                    c0 ^= p;
                    let carry1 = c1 & carry0;
                    c1 ^= carry0;
                    let carry2 = c2 & carry1;
                    c2 ^= carry1;
                    c3 |= carry2;
                }
                // Apply table[cur][count] with per-count equality masks.
                let m = cur_row[k];
                let mut next = 0u64;
                for (count, (&t_act, &t_ina)) in pt.table[1].iter().zip(&pt.table[0]).enumerate() {
                    if !t_act && !t_ina {
                        continue;
                    }
                    let b0 = if count & 1 != 0 { c0 } else { !c0 };
                    let b1 = if count & 2 != 0 { c1 } else { !c1 };
                    let b2 = if count & 4 != 0 { c2 } else { !c2 };
                    let b3 = if count & 8 != 0 { c3 } else { !c3 };
                    let eq = b0 & b1 & b2 & b3;
                    let src = match (t_act, t_ina) {
                        (true, true) => !0u64,
                        (true, false) => m,
                        _ => !m,
                    };
                    next |= eq & src;
                }
                next_bits[y * wpr + k] = next;
            }
            // Padding bits of the row's last word are not real cells; clear
            // them so a count-0-fires table row can't invent activity there.
            next_bits[y * wpr + wpr - 1] &= tail_mask;
        }

        let hl = self.history_limit;
        let mut active_count = 0u64;
        for y in 0..h {
            let base = y * wpr;
            for x in 0..w {
                let j = y * w + x;
                let cur = self.cells[j];
                let nb = (next_bits[base + x / 64] >> (x % 64)) & 1 == 1;
                let new_type = if nb { active } else { inactive };
                self.next_cells[j] = new_type;
                if hl > 0 {
                    let hb = j * hl;
                    let hd = self.history_heads[j] as usize;
                    self.history_data[hb + hd] = cur;
                    self.history_heads[j] = if hd + 1 == hl { 0 } else { (hd + 1) as u8 };
                    let c = self.history_counts[j] as usize;
                    if c < hl {
                        self.history_counts[j] = (c + 1) as u8;
                    }
                }
                if cur == new_type {
                    self.ages[j] = self.ages[j].saturating_add(1);
                } else {
                    self.ages[j] = 0;
                }
                active_count += nb as u64;
            }
        }

        // Same shape the per-cell path produces: count everything except the
        // dominant type (apply_counts back-fills it by subtraction).
        let dt = self.dominant_type;
        let mut count_map = TypeCounter::new();
        if active != dt {
            count_map.add_n(active, active_count);
        }
        if inactive != dt {
            count_map.add_n(inactive, (w * h) as u64 - active_count);
        }
        Some(count_map)
    }

    /// Advance the automaton by one step using double-buffering.
    ///
    /// When an [`crate::external::ExternalModel`] is attached, it drives the
    /// transition instead of the subrule engine.
    pub fn step(&mut self) {
        if self.model.is_some() {
            return self.step_external();
        }
        let width = self.width;
        let height = self.height;
        let total = width * height;
        let hl = self.history_limit;

        // The plan is an owned value (it copies what it needs out of the
        // rule), so it is built once here and reused by whichever path runs.
        let plan = Rule2DPlan::with_inactive(&self.rule, width, self.inactive);

        // Bit-parallel fast path for two-state threshold rules (see
        // PackedThreshold2D). Runs regardless of thread count — it is a
        // single-pass whole-grid stepper and deterministic by construction.
        // When it declines (foreign cell type on the grid), the scalar path
        // below picks up with the already-built plan.
        if total > 0
            && let Some(pt) = plan.packed
            && let Some(count_map) = self.step_packed(pt)
        {
            std::mem::swap(&mut self.cells, &mut self.next_cells);
            apply_counts(
                &mut self.counts_current,
                &mut self.peak_counts,
                &mut self.dominant_type,
                total as u64,
                &count_map,
            );
            self.step = self.step.saturating_add(1);
            return;
        }

        let nchunks = chunks_for_work(total.saturating_mul(plan.work_per_cell));
        let cells = &self.cells;
        let rule = &self.rule;
        let inactive = self.inactive;
        let dt = self.dominant_type;
        let seed = self.seed;
        let step = self.step;

        let count_map = if nchunks <= 1 {
            let mut out = OutChunk {
                start: 0,
                next_cells: &mut self.next_cells,
                ages: &mut self.ages,
                history_data: &mut self.history_data,
                history_heads: &mut self.history_heads,
                history_counts: &mut self.history_counts,
            };
            Self::step_chunk(
                cells, &mut out, hl, rule, &plan, inactive, dt, width, height, seed, step,
            )
        } else {
            let chunk = total.div_ceil(nchunks);
            let mut chunks = split_chunks(
                &mut self.next_cells,
                &mut self.ages,
                &mut self.history_data,
                &mut self.history_heads,
                &mut self.history_counts,
                hl,
                chunk,
            );
            // Persistent pool: no thread spawn/join per step.
            pool(nchunks).install(|| {
                chunks
                    .par_iter_mut()
                    .map(|c| {
                        Self::step_chunk(
                            cells, c, hl, rule, &plan, inactive, dt, width, height, seed, step,
                        )
                    })
                    .reduce(TypeCounter::new, |mut a, b| {
                        a.merge(&b);
                        a
                    })
            })
        };

        std::mem::swap(&mut self.cells, &mut self.next_cells);
        apply_counts(
            &mut self.counts_current,
            &mut self.peak_counts,
            &mut self.dominant_type,
            total as u64,
            &count_map,
        );
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
        if self.history_limit == 0 {
            return Vec::new();
        }
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
        self.cells
            .iter()
            .enumerate()
            .map(|(i, ct)| CellState {
                history: self.cell_history(i).into(),
                age_in_state: self.ages[i],
                history_limit: self.history_limit,
                current: *ct,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{CountOp, Neighborhood2D, Rule2DSubrule};
    use crate::threads::{
        clear_min_work_per_chunk_override, clear_thread_override, set_min_work_per_chunk_override,
        set_thread_override,
    };
    use serde_json::json;

    #[test]
    fn deserialize_and_debug_paths_work() {
        let inactive = CellType::inactive();
        let a = CellType::from("A");
        let counts: std::collections::HashMap<Spur, u64> =
            [(a.0, 3u64), (inactive.0, 1u64)].into_iter().collect();
        let peaks = counts.clone();

        let v = json!({
            "width": 2,
            "height": 2,
            "history_limit": 2,
            "cell_states": [
                {"history": ["Inactive"], "age_in_state": 1, "history_limit": 2, "current": "A"},
                {"history": ["A"], "age_in_state": 0, "history_limit": 2, "current": "A"},
                {"history": [], "age_in_state": 2, "history_limit": 2, "current": "A"},
                {"history": ["A"], "age_in_state": 0, "history_limit": 2, "current": "Inactive"}
            ],
            "step": 3,
            "rule": {"subrules": []},
            "counts_current": serde_json::to_value(counts).unwrap(),
            "peak_counts": serde_json::to_value(peaks).unwrap(),
            "inactive": "Inactive"
        });

        let g: Grid2D = serde_json::from_value(v).expect("deserialize Grid2D");
        assert_eq!(g.width, 2);
        assert_eq!(g.height, 2);
        assert_eq!(g.step, 3);
        let dbg = format!("{:?}", g);
        assert!(dbg.contains("Grid2D"));
    }

    #[test]
    fn transition_updates_age_and_history_count_branches() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D { subrules: vec![] };
        let mut g = Grid2D::new(1, 1, 1, vec![a], rule);

        assert!(g.transition_state_and_buffer(0, &a).is_none());
        assert_eq!(g.cell_age(0), 1);
        assert_eq!(g.history_counts[0], 1);

        assert!(g.transition_state_and_buffer(0, &b).is_none());
        assert_eq!(g.cell_age(0), 0);
        assert_eq!(g.history_counts[0], 1);
    }

    #[test]
    fn randomness_one_skips_rule_on_edge_path() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                Some(1.0),
                None,
            )],
        };
        // 1x1 grid always uses edge path.
        let mut g = Grid2D::new(1, 1, 0, vec![a], rule);
        g.step();
        assert_eq!(g.cell_type(0), CellType::inactive());
    }

    #[test]
    fn transition_path_without_history_limit_is_exercised() {
        let a = CellType::from("A");
        let mut g = Grid2D::new(1, 1, 0, vec![a], Rule2D { subrules: vec![] });
        assert!(
            g.transition_state_and_buffer(0, &CellType::inactive())
                .is_none()
        );
        assert!(g.history_data.is_empty());
    }

    #[test]
    fn seeded_randomness_is_deterministic_and_honours_zero_and_one() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let make = |r: Option<f64>| Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a,
                a,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                r,
                None,
            )],
        };
        let cells = vec![a; 9];
        let inactive = CellType::inactive();

        // Same seed and step: same answer, interior and edge alike.
        let rule = make(Some(0.5));
        let plan = Rule2DPlan::with_inactive(&rule, 3, inactive);
        let i1 = Grid2D::next_type_interior(&cells, &rule, &plan, inactive, 4, 5, 2);
        let i2 = Grid2D::next_type_interior(&cells, &rule, &plan, inactive, 4, 5, 2);
        assert_eq!(i1, i2);
        let e1 = Grid2D::next_type_edge(&cells, &rule, inactive, 3, 3, 0, 0, 0, 5, 2);
        let e2 = Grid2D::next_type_edge(&cells, &rule, inactive, 3, 3, 0, 0, 0, 5, 2);
        assert_eq!(e1, e2);

        // A fair coin over many seeds lands on both sides.
        let outcomes: std::collections::HashSet<CellType> = (0..64u64)
            .map(|seed| Grid2D::next_type_interior(&cells, &rule, &plan, inactive, 4, seed, 0))
            .collect();
        assert_eq!(outcomes.len(), 2, "both skip and apply must occur");

        // randomness 0.0 never skips; 1.0 always skips.
        let never = make(Some(0.0));
        let plan_never = Rule2DPlan::with_inactive(&never, 3, inactive);
        let always = make(Some(1.0));
        let plan_always = Rule2DPlan::with_inactive(&always, 3, inactive);
        for seed in 0..16u64 {
            assert_eq!(
                Grid2D::next_type_interior(&cells, &never, &plan_never, inactive, 4, seed, 0),
                b
            );
            assert_eq!(
                Grid2D::next_type_edge(&cells, &never, inactive, 3, 3, 0, 0, 0, seed, 0),
                b
            );
            assert_eq!(
                Grid2D::next_type_interior(&cells, &always, &plan_always, inactive, 4, seed, 0),
                inactive
            );
            assert_eq!(
                Grid2D::next_type_edge(&cells, &always, inactive, 3, 3, 0, 0, 0, seed, 0),
                inactive
            );
        }
    }

    #[test]
    fn deserialize_fallback_error_and_rng_continue_paths() {
        let v = serde_json::json!({
            "width": 1,
            "height": 1,
            "history_limit": 0,
            "cell_states": [{"history": [], "age_in_state": 0, "history_limit": 0, "current": "Inactive"}],
            "step": 0,
            "rule": {"subrules": []},
            "counts_current": {},
            "peak_counts": {},
            "inactive": "Inactive"
        });
        let deser: Grid2D = serde_json::from_value(v).expect("deserialize with empty maps");
        assert_eq!(deser.dominant_type, CellType::inactive());

        let bad = serde_json::from_str::<Grid2D>("{\"width\":\"nope\"}");
        assert!(bad.is_err());

        // randomness=1.0 guarantees the rng<r branch and continue path are executed.
        let a = CellType::from("A");
        let b = CellType::from("B");
        let sr = Rule2DSubrule::new(
            a,
            a,
            0,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            b,
            Some(1.0),
            None,
        );
        let rule = Rule2D { subrules: vec![sr] };
        let cells = vec![a; 9];
        let plan = Rule2DPlan::with_inactive(&rule, 3, CellType::inactive());
        let interior = Grid2D::next_type_interior(
            &cells,
            &rule,
            &plan,
            CellType::inactive(),
            4,
            7,
            0,
        );
        let edge = Grid2D::next_type_edge(
            &cells,
            &rule,
            CellType::inactive(),
            3,
            3,
            0,
            0,
            0,
            9,
            0,
        );
        assert_eq!(interior, CellType::inactive());
        assert_eq!(edge, CellType::inactive());

        // Force deterministic continue in both paths via r>1 (internal-path coverage).
        let sr_force = Rule2DSubrule::new(
            a,
            a,
            0,
            CountOp::Gt,
            1,
            Neighborhood2D::Moore,
            b,
            Some(2.0),
            None,
        );
        let rule_force = Rule2D {
            subrules: vec![sr_force],
        };
        let plan_force = Rule2DPlan::with_inactive(&rule_force, 3, CellType::inactive());
        let interior2 = Grid2D::next_type_interior(
            &cells,
            &rule_force,
            &plan_force,
            CellType::inactive(),
            4,
            11,
            0,
        );
        let edge2 = Grid2D::next_type_edge(
            &cells,
            &rule_force,
            CellType::inactive(),
            3,
            3,
            0,
            0,
            0,
            13,
            0,
        );
        assert_eq!(interior2, CellType::inactive());
        assert_eq!(edge2, CellType::inactive());
    }

    #[test]
    fn out_of_bounds_history_full_and_parallel_step_paths() {
        let a = CellType::from("A");
        let b = CellType::from("B");

        let mut g_hist = Grid2D::new(1, 1, 2, vec![a], Rule2D { subrules: vec![] });
        assert!(g_hist.transition_state_and_buffer(99, &b).is_some());
        assert!(g_hist.transition_state_and_buffer(0, &b).is_none());
        assert!(g_hist.transition_state_and_buffer(0, &a).is_none());
        let hist = g_hist.cell_history(0);
        assert_eq!(hist.len(), 2);

        set_thread_override(2);
        set_min_work_per_chunk_override(1);
        let mut g_parallel = Grid2D::new(4, 4, 1, vec![a; 16], Rule2D { subrules: vec![] });
        g_parallel.step();
        clear_min_work_per_chunk_override();
        clear_thread_override();
    }

    /// A <-> B coin flip that ignores neighbours: A becomes B (and B becomes
    /// A) with probability 0.5 each step, otherwise it stays. Anything else,
    /// including Inactive, matches no subrule and stays Inactive.
    fn coin_flip_rule() -> Rule2D {
        let a = CellType::from("A");
        let b = CellType::from("B");
        Rule2D {
            subrules: vec![
                Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, Some(0.5), None),
                Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, a, None, None),
                Rule2DSubrule::new(b, b, 0, CountOp::Gt, 1, Neighborhood2D::Moore, a, Some(0.5), None),
                Rule2DSubrule::new(b, b, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, None, None),
            ],
        }
    }

    fn coin_grid(w: usize, h: usize, steps: u64) -> Grid2D {
        let a = CellType::from("A");
        let mut g = Grid2D::new(w, h, 3, vec![a; w * h], coin_flip_rule()).with_seed(7);
        for _ in 0..steps {
            g.step();
        }
        g
    }

    #[test]
    fn resize_grow_keeps_the_overlap_and_pads_with_fresh_inactive_cells() {
        let before = coin_grid(3, 2, 5);
        let mut g = before.clone();
        g.resize(5, 4).unwrap();
        assert_eq!((g.width, g.height, g.step, g.seed), (5, 4, 5, 7));
        for y in 0..4 {
            for x in 0..5 {
                let i = y * 5 + x;
                if x < 3 && y < 2 {
                    let o = y * 3 + x;
                    assert_eq!(g.cell_type(i), before.cell_type(o), "type ({x},{y})");
                    assert_eq!(g.cell_age(i), before.cell_age(o), "age ({x},{y})");
                    assert_eq!(g.cell_history(i), before.cell_history(o), "history ({x},{y})");
                } else {
                    assert_eq!(g.cell_type(i), g.inactive, "new cell ({x},{y})");
                    assert_eq!(g.cell_age(i), 0);
                    assert!(g.cell_history(i).is_empty());
                }
            }
        }
        let total: u64 = g.counts_current.values().sum();
        assert_eq!(total, 20);
        assert_eq!(g.counts_current[&g.inactive.0], 14);
        assert_eq!(g.next_cells.len(), 20);
    }

    #[test]
    fn resize_shrink_crops_right_and_bottom() {
        let before = coin_grid(6, 4, 5);
        let mut g = before.clone();
        g.resize(2, 3).unwrap();
        assert_eq!((g.width, g.height), (2, 3));
        for y in 0..3 {
            for x in 0..2 {
                assert_eq!(g.cell_type(y * 2 + x), before.cell_type(y * 6 + x));
                assert_eq!(g.cell_history(y * 2 + x), before.cell_history(y * 6 + x));
            }
        }
        // A peak that happened stays a peak, even though fewer cells remain.
        for (k, v) in &before.peak_counts {
            assert!(g.peak_counts[k] >= *v);
        }
    }

    #[test]
    fn resize_to_the_same_size_changes_nothing() {
        let before = coin_grid(4, 4, 3);
        let mut g = before.clone();
        g.resize(4, 4).unwrap();
        for i in 0..16 {
            assert_eq!(g.cell_type(i), before.cell_type(i));
            assert_eq!(g.cell_history(i), before.cell_history(i));
        }
        assert_eq!(g.step, before.step);
    }

    #[test]
    fn resize_to_zero_is_refused_and_leaves_the_grid_unchanged() {
        let mut g = coin_grid(4, 4, 3);
        let err = g.resize(0, 4).unwrap_err();
        assert!(matches!(err, crate::resize::ResizeError::ZeroSize { .. }));
        assert_eq!((g.width, g.height, g.step), (4, 4, 3));
        assert_eq!(g.cells().len(), 16);
    }

    #[test]
    fn resize_with_no_history_grows_and_shrinks_and_steps_without_panicking() {
        // history_limit 0 takes the `if hl > 0` false branch inside resize:
        // the history buffers stay empty instead of being remapped.
        let a = CellType::from("A");
        let mut before = Grid2D::new(3, 2, 0, vec![a; 6], coin_flip_rule()).with_seed(7);
        for _ in 0..5 {
            before.step();
        }
        assert!(before.history_data.is_empty());

        let mut grown = before.clone();
        grown.resize(5, 4).unwrap();
        assert_eq!((grown.width, grown.height), (5, 4));
        for y in 0..2 {
            for x in 0..3 {
                let i = y * 5 + x;
                let o = y * 3 + x;
                assert_eq!(grown.cell_type(i), before.cell_type(o), "type ({x},{y})");
                assert_eq!(grown.cell_age(i), before.cell_age(o), "age ({x},{y})");
            }
        }
        assert!(grown.history_data.is_empty());
        grown.step(); // must not panic

        let mut shrunk = before.clone();
        shrunk.resize(2, 1).unwrap();
        assert_eq!((shrunk.width, shrunk.height), (2, 1));
        for x in 0..2 {
            assert_eq!(shrunk.cell_type(x), before.cell_type(x));
            assert_eq!(shrunk.cell_age(x), before.cell_age(x));
        }
        assert!(shrunk.history_data.is_empty());
        shrunk.step(); // must not panic
    }

    #[test]
    fn a_height_only_resize_keeps_every_surviving_cells_random_stream() {
        let mut reference = coin_grid(4, 4, 4);
        let mut g = coin_grid(4, 4, 4);
        g.resize(4, 6).unwrap(); // rows added below: flat indices 0..16 unchanged
        for _ in 0..6 {
            reference.step();
            g.step();
        }
        for i in 0..16 {
            assert_eq!(g.cell_type(i), reference.cell_type(i), "cell {i}");
            assert_eq!(g.cell_age(i), reference.cell_age(i), "age {i}");
        }
    }

    #[test]
    fn a_width_resize_is_still_deterministic() {
        let mut g1 = coin_grid(4, 4, 4);
        let mut g2 = coin_grid(4, 4, 4);
        g1.resize(6, 4).unwrap();
        g2.resize(6, 4).unwrap();
        for _ in 0..6 {
            g1.step();
            g2.step();
        }
        assert_eq!(g1.cells(), g2.cells());
    }

    #[test]
    fn resize_runs_the_default_model_hook() {
        let mut g = coin_grid(4, 4, 0);
        g.attach_model(Box::new(crate::external::tests::ConstModel {
            out_name: "A".into(),
            event_target: None,
            attached: false,
            threshold: 1.0,
        }))
        .unwrap();
        g.resize(5, 5).unwrap();
        assert!(g.model.is_some());
    }

    #[test]
    fn a_model_that_refuses_leaves_the_grid_unchanged() {
        let mut g = coin_grid(4, 4, 2);
        g.attach_model(Box::new(crate::external::tests::NoResizeModel)).unwrap();
        let cells_before = g.cells().to_vec();
        let err = g.resize(5, 5).unwrap_err();
        assert!(matches!(err, crate::resize::ResizeError::Model(_)));
        assert_eq!((g.width, g.height, g.step), (4, 4, 2));
        assert_eq!(g.cells(), &cells_before[..]);
        assert!(g.model.is_some());
    }
}
