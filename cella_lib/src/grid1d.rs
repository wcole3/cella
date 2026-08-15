//! 1D grid implementation.

use std::io::Error;
use lasso2::Spur;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};
use crate::chunking::{split_chunks, OutChunk};
use crate::types::{CellState, CellType};
use crate::rules::{apply_counts, PackedWolfram, Rule1D, Rule1DPlan, Rule1DSubrule, Sub1DPlan, TypeCounter};
use crate::threads::{chunks_for_work, pool};

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
        if idx >= self.width {
            Some(Error::new(std::io::ErrorKind::InvalidInput, "Index out of bounds"))
        } else {
            self.transition_cell(idx, *new_type);
            None
        }
    }

    /// Wolfram bit index for a cell whose whole `2n+1` window is in bounds:
    /// straight reads, no per-slot bounds test and no temporary window array.
    ///
    /// The `n` arms are spelled out rather than looped so each window folds into
    /// straight-line code — a loop bounded by the runtime `n` does not unroll and
    /// measurably costs more than the bounds checks it removes.
    /// `n <= 2` windows index at most bit 31, so the shift runs on the plan's
    /// 64-bit `code_lo`; only `n = 3` (up to bit 127) pays for a 128-bit shift.
    /// The caller has already checked `plan.valid`, so no `n` re-validation.
    #[inline]
    fn applies_interior(s: &Rule1DSubrule, plan: &Sub1DPlan, cells: &[CellType], idx: usize,
                        current: CellType) -> bool {
        let crit = s.criteria_type;
        let hit = |j: usize| (cells[j] == crit) as u64;
        // The centre slot is the caller's already-loaded `current`, not a re-read.
        let mid = (current == crit) as u64;
        let bits: u64 = match s.n {
            1 => (hit(idx - 1) << 2) | (mid << 1) | hit(idx + 1),
            2 => (hit(idx - 2) << 4) | (hit(idx - 1) << 3) | (mid << 2)
                 | (hit(idx + 1) << 1) | hit(idx + 2),
            _ => {
                let bits = (hit(idx - 3) << 6) | (hit(idx - 2) << 5) | (hit(idx - 1) << 4)
                    | (mid << 3) | (hit(idx + 1) << 2) | (hit(idx + 2) << 1)
                    | hit(idx + 3);
                return (s.wolfram_code >> bits) & 1u128 == 1u128;
            }
        };
        (plan.code_lo >> bits) & 1u64 == 1u64
    }

    /// Same as `applies_interior` but treats out-of-bounds slots as `inactive`.
    #[inline]
    fn applies_edge(s: &Rule1DSubrule, cells: &[CellType], inactive: CellType,
                    width: usize, idx: usize) -> bool {
        let n = s.n as isize;
        let crit = s.criteria_type;
        let mut bits: u128 = 0;
        for k in -n..=n {
            let j = idx as isize + k;
            let t = if j < 0 || j as usize >= width { inactive } else { cells[j as usize] };
            bits = (bits << 1) | ((t == crit) as u128);
        }
        (s.wolfram_code >> bits) & 1u128 == 1u128
    }

    /// Evaluate subrules for a cell at least `rule.n_max()` from either end, so
    /// every window slot is in bounds.
    ///
    /// Subrules with `n < 1` or `n > 3` cannot be encoded in a `u128` window and
    /// never match; [`Rule1DSubrule::validate`] rejects them, and `applies_*`
    /// reports no match for them here.
    #[inline]
    fn next_type_interior(cells: &[CellType], rule: &Rule1D, plan: &Rule1DPlan, inactive: CellType,
                          idx: usize, mut rng: Option<&mut SmallRng>) -> CellType {
        let current_type = cells[idx];
        for (s, ps) in rule.subrules.iter().zip(&plan.subs) {
            if current_type != s.current_type || !ps.valid { continue; }
            if !Self::applies_interior(s, ps, cells, idx, current_type) { continue; }
            if let Some(r) = s.randomness {
                if let Some(rng) = rng.as_deref_mut() {
                    if rng.r#gen::<f64>() < r { continue; }
                }
            }
            return s.output_type;
        }
        inactive
    }

    /// Evaluate subrules for a cell near either end, treating out-of-bounds
    /// window slots as `inactive`.
    ///
    #[inline]
    fn next_type_edge(cells: &[CellType], rule: &Rule1D, plan: &Rule1DPlan, inactive: CellType,
                      width: usize, idx: usize, mut rng: Option<&mut SmallRng>) -> CellType {
        let current_type = cells[idx];
        for (s, ps) in rule.subrules.iter().zip(&plan.subs) {
            if current_type != s.current_type || !ps.valid { continue; }
            if !Self::applies_edge(s, cells, inactive, width, idx) { continue; }
            if let Some(r) = s.randomness {
                if let Some(rng) = rng.as_deref_mut() {
                    if rng.r#gen::<f64>() < r { continue; }
                }
            }
            return s.output_type;
        }
        inactive
    }

    /// Compute the next state for one chunk, writing into disjoint output slices.
    #[allow(clippy::too_many_arguments)]
    fn step_chunk(
        cells: &[CellType], out: &mut OutChunk<'_>, history_limit: usize,
        rule: &Rule1D, plan: &Rule1DPlan, pad: usize, needs_rng: bool,
        inactive: CellType, dt: CellType, width: usize,
    ) -> TypeCounter {
        let mut count_map = TypeCounter::new();
        // Only pay for RNG setup when a subrule actually draws from it.
        let mut rng = if needs_rng { Some(SmallRng::from_entropy()) } else { None };
        let start = out.start;
        // Reborrow into locals: indexing through `&mut OutChunk` makes the loop
        // reload each slice's pointer and length from the struct on every access.
        let next_cells = &mut *out.next_cells;
        let ages = &mut *out.ages;
        let history_data = &mut *out.history_data;
        let history_heads = &mut *out.history_heads;
        let history_counts = &mut *out.history_counts;
        let hi = width.saturating_sub(pad);
        for local in 0..next_cells.len() {
            let idx = start + local;
            let new_type = if idx >= pad && idx < hi {
                Self::next_type_interior(cells, rule, plan, inactive, idx, rng.as_mut())
            } else {
                Self::next_type_edge(cells, rule, plan, inactive, width, idx, rng.as_mut())
            };
            next_cells[local] = new_type;
            let cur = cells[idx];
            if history_limit > 0 {
                let base_hist = local * history_limit;
                let h = history_heads[local] as usize;
                history_data[base_hist + h] = cur;
                // `% history_limit` would be a hardware divide (the limit is a
                // runtime value) on every cell of every step; h is always
                // < history_limit, so a compare suffices.
                history_heads[local] = if h + 1 == history_limit { 0 } else { (h + 1) as u8 };
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

    /// The fast step for a [`PackedWolfram`] rule: the row is stored one
    /// **bit** per cell (1 = `active`, 0 = inactive) inside 64-bit integers,
    /// so one machine instruction processes 64 cells at once.
    ///
    /// Returns `None` when the fast path can't be used — some cell is neither
    /// `active` nor inactive (e.g. the user painted a third type). The caller
    /// then runs the normal scalar path instead, so the result is always
    /// identical either way; this check runs every step.
    ///
    /// # How one word is stepped
    ///
    /// Every cell needs to see three cells: its left neighbor, itself, and
    /// its right neighbor. With the row packed into words we can hand *all*
    /// cells their neighbors at once by shifting whole words:
    ///
    /// ```text
    /// m = the word itself         -> bit j is cell j
    /// l = m shifted left by 1     -> bit j is cell j-1 (left neighbor)
    /// r = m shifted right by 1    -> bit j is cell j+1 (right neighbor)
    /// ```
    ///
    /// (The bit that falls off the end of one word is carried in from the
    /// neighboring word; at the ends of the whole row, zeros come in — which
    /// matches the scalar rule "out of bounds counts as inactive".)
    ///
    /// Now each cell's window is the trio of bits `(l, m, r)` sitting in the
    /// same position of those three words. The Wolfram table says which of
    /// the 8 possible trios produce an active cell. For each such trio, we
    /// build a mask that is 1 exactly where that trio occurs — e.g. for the
    /// pattern `110` (left on, middle on, right off) the mask is
    /// `l & m & !r` — and OR all those masks together. That OR is the entire
    /// next row, computed ~64 cells at a time.
    ///
    /// The per-cell bookkeeping (writing next types, ages, history, counts)
    /// still runs as a normal loop afterwards; only the rule evaluation is
    /// bit-parallel.
    fn step_packed(&mut self, pw: PackedWolfram) -> Option<TypeCounter> {
        let width = self.width;
        let active = pw.active;
        let inactive = self.inactive;
        if self.cells.iter().any(|&c| c != active && c != inactive) {
            return None;
        }
        let nwords = width.div_ceil(64);
        let mut bits = vec![0u64; nwords];
        for (j, &c) in self.cells.iter().enumerate() {
            bits[j / 64] |= ((c == active) as u64) << (j % 64);
        }
        let code = pw.code;
        let mut next_bits = vec![0u64; nwords];
        for k in 0..nwords {
            let m = bits[k];
            let l = (m << 1) | if k > 0 { bits[k - 1] >> 63 } else { 0 };
            let r = (m >> 1) | if k + 1 < nwords { bits[k + 1] << 63 } else { 0 };
            let mut out = 0u64;
            for p in 0..8u8 {
                if (code >> p) & 1 == 1 {
                    let lp = if p & 4 != 0 { l } else { !l };
                    let mp = if p & 2 != 0 { m } else { !m };
                    let rp = if p & 1 != 0 { r } else { !r };
                    out |= lp & mp & rp;
                }
            }
            next_bits[k] = out;
        }
        // Bits past `width` in the last word are not real cells; clear them so
        // the pattern-000 case can't invent activity there.
        let tail = width % 64;
        if tail != 0 {
            next_bits[nwords - 1] &= (1u64 << tail) - 1;
        }

        let hl = self.history_limit;
        let mut active_count = 0u64;
        for j in 0..width {
            let cur = self.cells[j];
            let nb = (next_bits[j / 64] >> (j % 64)) & 1 == 1;
            let new_type = if nb { active } else { inactive };
            self.next_cells[j] = new_type;
            if hl > 0 {
                let base = j * hl;
                let h = self.history_heads[j] as usize;
                self.history_data[base + h] = cur;
                self.history_heads[j] = if h + 1 == hl { 0 } else { (h + 1) as u8 };
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

        // Same shape the per-cell path produces: count everything except the
        // dominant type (which apply_counts back-fills by subtraction).
        let dt = self.dominant_type;
        let mut count_map = TypeCounter::new();
        if active != dt {
            count_map.add_n(active, active_count);
        }
        if inactive != dt {
            count_map.add_n(inactive, width as u64 - active_count);
        }
        Some(count_map)
    }

    /// Advance the automaton by one step using double-buffering.
    pub fn step(&mut self) {
        let width = self.width;
        let hl = self.history_limit;
        let pad = self.rule.n_max() as usize;
        let needs_rng = self.rule.needs_rng();
        // Nominal neighbor visits per cell: one 2n+1 window per subrule.
        let work_per_cell: usize = self.rule.subrules.iter()
            .map(|s| 2 * s.n as usize + 1).sum::<usize>().max(1);
        let nchunks = chunks_for_work(width.saturating_mul(work_per_cell));

        // Bit-parallel fast path for pure two-state Wolfram rules (serial
        // only — word carries don't cross chunk boundaries). Falls back to
        // the scalar path when the grid holds any foreign cell type.
        // The plan is an owned value (it copies what it needs out of the
        // rule), so it is built once here and reused by whichever path runs.
        let plan = Rule1DPlan::new(&self.rule, self.inactive);

        if nchunks <= 1 && width > 0 {
            if let Some(pw) = plan.packed {
                if let Some(count_map) = self.step_packed(pw) {
                    std::mem::swap(&mut self.cells, &mut self.next_cells);
                    apply_counts(&mut self.counts_current, &mut self.peak_counts,
                        &mut self.dominant_type, width as u64, &count_map);
                    self.step = self.step.saturating_add(1);
                    return;
                }
            }
        }

        let plan = &plan;
        let cells = &self.cells;
        let rule = &self.rule;
        let inactive = self.inactive;
        let dt = self.dominant_type;

        let count_map = if nchunks <= 1 {
            let mut out = OutChunk {
                start: 0,
                next_cells: &mut self.next_cells,
                ages: &mut self.ages,
                history_data: &mut self.history_data,
                history_heads: &mut self.history_heads,
                history_counts: &mut self.history_counts,
            };
            Self::step_chunk(cells, &mut out, hl, rule, plan, pad, needs_rng, inactive, dt, width)
        } else {
            let chunk = width.div_ceil(nchunks);
            let mut chunks = split_chunks(
                &mut self.next_cells, &mut self.ages,
                &mut self.history_data, &mut self.history_heads, &mut self.history_counts,
                hl, chunk,
            );
            // Persistent pool: no thread spawn/join per step.
            pool(nchunks).install(|| {
                chunks.par_iter_mut()
                    .map(|c| Self::step_chunk(cells, c, hl, rule, plan, pad, needs_rng, inactive, dt, width))
                    .reduce(TypeCounter::new, |mut a, b| { a.merge(&b); a })
            })
        };

        std::mem::swap(&mut self.cells, &mut self.next_cells);
        apply_counts(&mut self.counts_current, &mut self.peak_counts,
            &mut self.dominant_type, width as u64, &count_map);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threads::{clear_min_work_per_chunk_override, clear_thread_override, set_min_work_per_chunk_override, set_thread_override};
    use serde_json::json;
    use std::collections::VecDeque;

    #[test]
    fn deserialize_and_debug_paths_work() {
        let inactive = CellType::inactive();
        let a = CellType::from("A");
        let counts: std::collections::HashMap<Spur, u64> = [(a.0, 2u64), (inactive.0, 1u64)].into_iter().collect();
        let peaks = counts.clone();

        let v = json!({
            "width": 3,
            "history_limit": 2,
            "cell_states": [
                {"history": ["Inactive"], "age_in_state": 1, "history_limit": 2, "current": "A"},
                {"history": ["A"], "age_in_state": 0, "history_limit": 2, "current": "A"},
                {"history": [], "age_in_state": 2, "history_limit": 2, "current": "Inactive"}
            ],
            "step": 7,
            "rule": {"subrules": []},
            "counts_current": serde_json::to_value(counts).unwrap(),
            "peak_counts": serde_json::to_value(peaks).unwrap(),
            "inactive": "Inactive"
        });

        let g: Grid1D = serde_json::from_value(v).expect("deserialize Grid1D");
        assert_eq!(g.width, 3);
        assert_eq!(g.step, 7);
        assert_eq!(g.cell_type(0), a);
        let dbg = format!("{:?}", g);
        assert!(dbg.contains("Grid1D"));
    }

    #[test]
    fn transition_updates_age_and_history_count_branches() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule1D { subrules: vec![] };
        let mut g = Grid1D::new(1, 1, vec![a], rule);

        assert!(g.transition_state_and_buffer(0, &a).is_none());
        assert_eq!(g.cell_age(0), 1);
        assert_eq!(g.history_counts[0], 1);

        // With history_limit=1, count is already full: the c < hl branch is now false.
        assert!(g.transition_state_and_buffer(0, &b).is_none());
        assert_eq!(g.cell_age(0), 0);
        assert_eq!(g.history_counts[0], 1);
    }

    #[test]
    fn randomness_one_skips_rule_for_interior_and_edge_cells() {
        let x = CellType::from("X");
        let y = CellType::from("Y");

        let sub = Rule1DSubrule {
            current_type: x,
            criteria_type: x,
            wolfram_code: u128::MAX,
            n: 1,
            randomness: Some(1.0),
            output_type: y,
        };
        let rule = Rule1D { subrules: vec![sub] };

        // width=3: middle cell takes interior path
        let mut interior = Grid1D::new(3, 0, vec![x, x, x], rule.clone());
        interior.step();
        assert_eq!(interior.cell_type(1), CellType::inactive());

        // width=1: only cell takes edge path
        let mut edge = Grid1D::new(1, 0, vec![x], rule);
        edge.step();
        assert_eq!(edge.cell_type(0), CellType::inactive());
    }

    #[test]
    fn to_cell_states_preserves_history_fifo() {
        let a = CellType::from("A");
        let rule = Rule1D { subrules: vec![] };
        let mut g = Grid1D::new(1, 2, vec![a], rule);
        g.transition_state_and_buffer(0, &CellType::inactive());
        let states = g.to_cell_states();
        assert_eq!(states.len(), 1);
        let hist: VecDeque<CellType> = states[0].history.clone();
        assert_eq!(hist.len(), 1);
    }

    #[test]
    fn transition_path_without_history_limit_is_exercised() {
        let a = CellType::from("A");
        let mut g = Grid1D::new(1, 0, vec![a], Rule1D { subrules: vec![] });
        assert!(g.transition_state_and_buffer(0, &CellType::inactive()).is_none());
        assert!(g.history_data.is_empty());
    }

    #[test]
    fn invalid_n_and_rng_none_paths_are_exercised() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let cells = vec![a, a, a];

        let invalid_n = Rule1DSubrule {
            current_type: a,
            criteria_type: a,
            wolfram_code: u128::MAX,
            n: 4,
            randomness: None,
            output_type: b,
        };
        // Invalid n is rejected at plan level; the subrule can never match.
        let invalid_rule = Rule1D { subrules: vec![invalid_n.clone()] };
        let invalid_plan = Rule1DPlan::new(&invalid_rule, CellType::inactive());
        assert!(!invalid_plan.subs[0].valid);
        assert_eq!(
            Grid1D::next_type_interior(&cells, &invalid_rule, &invalid_plan, CellType::inactive(), 1, None),
            CellType::inactive()
        );

        let random_sub = Rule1DSubrule { randomness: Some(0.5), n: 1, ..invalid_n.clone() };
        let rule = Rule1D { subrules: vec![random_sub] };
        let plan = Rule1DPlan::new(&rule, CellType::inactive());
        // Call internals directly with rng=None to cover that branch.
        let got_interior = Grid1D::next_type_interior(&cells, &rule, &plan, CellType::inactive(), 1, None);
        assert_eq!(got_interior, b);
        let got_edge = Grid1D::next_type_edge(&cells, &rule, &plan, CellType::inactive(), cells.len(), 0, None);
        assert_eq!(got_edge, b);
    }

    #[test]
    fn zero_history_accessors_and_deserialize_fallback_paths() {
        let inactive = CellType::inactive();
        let a = CellType::from("A");
        let mut g = Grid1D::new(2, 0, vec![a, inactive], Rule1D { subrules: vec![] });
        assert!(g.cell_history(0).is_empty());
        assert!(g.cell_history(1).is_empty());

        let v = serde_json::json!({
            "width": 2,
            "history_limit": 0,
            "cell_states": [
                {"history": [], "age_in_state": 0, "history_limit": 0, "current": "A"},
                {"history": [], "age_in_state": 0, "history_limit": 0, "current": "Inactive"}
            ],
            "step": 0,
            "rule": {"subrules": []},
            "counts_current": {},
            "peak_counts": {},
            "inactive": "Inactive"
        });
        let deser: Grid1D = serde_json::from_value(v).expect("deserialize with empty maps");
        assert_eq!(deser.dominant_type, CellType::inactive());

        let bad = serde_json::from_str::<Grid1D>("{\"width\":\"nope\"}");
        assert!(bad.is_err());

        // Force rng<r continue path in next_type_edge with certainty.
        let mut rng = SmallRng::seed_from_u64(1);
        let sub = Rule1DSubrule {
            current_type: CellType::from("A"),
            criteria_type: CellType::from("A"),
            wolfram_code: u128::MAX,
            n: 1,
            randomness: Some(1.0),
            output_type: CellType::from("B"),
        };
        let rng_rule = Rule1D { subrules: vec![sub] };
        let rng_plan = Rule1DPlan::new(&rng_rule, CellType::inactive());
        let out = Grid1D::next_type_edge(&[CellType::from("A")], &rng_rule, &rng_plan, CellType::inactive(), 1, 0, Some(&mut rng));
        assert_eq!(out, CellType::inactive());

        // n < 1 is rejected at plan level, so the subrule never matches.
        let invalid = Rule1DSubrule {
            current_type: CellType::from("A"),
            criteria_type: CellType::from("A"),
            wolfram_code: 1,
            n: 0,
            randomness: None,
            output_type: CellType::from("B"),
        };
        let invalid_rule = Rule1D { subrules: vec![invalid] };
        let invalid_plan = Rule1DPlan::new(&invalid_rule, CellType::inactive());
        assert!(!invalid_plan.subs[0].valid);
        assert_eq!(
            Grid1D::next_type_edge(&[CellType::from("A")], &invalid_rule, &invalid_plan, CellType::inactive(), 1, 0, None),
            CellType::inactive()
        );
        g.step();
    }

    #[test]
    fn out_of_bounds_n3_history_full_and_parallel_step_paths() {
        let a = CellType::from("A");
        let b = CellType::from("B");

        let sub_n3 = Rule1DSubrule {
            current_type: a,
            criteria_type: a,
            wolfram_code: u128::MAX,
            n: 3,
            randomness: None,
            output_type: b,
        };
        let cells = vec![a; 7];
        let n3_rule = Rule1D { subrules: vec![sub_n3.clone()] };
        let n3_plan = Rule1DPlan::new(&n3_rule, CellType::inactive());
        assert!(Grid1D::applies_interior(&sub_n3, &n3_plan.subs[0], &cells, 3, a));

        let mut g_hist = Grid1D::new(1, 2, vec![a], Rule1D { subrules: vec![] });
        assert!(g_hist.transition_state_and_buffer(99, &b).is_some());
        assert!(g_hist.transition_state_and_buffer(0, &b).is_none());
        assert!(g_hist.transition_state_and_buffer(0, &a).is_none());
        let hist = g_hist.cell_history(0);
        assert_eq!(hist.len(), 2);

        set_thread_override(2);
        set_min_work_per_chunk_override(1);
        let mut g_parallel = Grid1D::new(8, 1, vec![a; 8], Rule1D { subrules: vec![] });
        g_parallel.step();
        clear_min_work_per_chunk_override();
        clear_thread_override();
    }

    #[test]
    fn empty_initial_uses_inactive_dominant_and_rng_continue_interior() {
        let g_empty = Grid1D::new(0, 0, vec![], Rule1D { subrules: vec![] });
        assert_eq!(g_empty.dominant_type, CellType::inactive());

        let a = CellType::from("A");
        let b = CellType::from("B");
        let sub = Rule1DSubrule {
            current_type: a,
            criteria_type: a,
            wolfram_code: u128::MAX,
            n: 1,
            randomness: Some(1.0),
            output_type: b,
        };
        let rule = Rule1D { subrules: vec![sub] };
        let plan = Rule1DPlan::new(&rule, CellType::inactive());
        let cells = vec![a, a, a];
        let mut rng = SmallRng::seed_from_u64(123);
        let out = Grid1D::next_type_interior(&cells, &rule, &plan, CellType::inactive(), 1, Some(&mut rng));
        assert_eq!(out, CellType::inactive());

        // Use r>1 to make the continue branch deterministic for internal-path coverage.
        let sub_force = Rule1DSubrule { randomness: Some(2.0), ..rule.subrules[0].clone() };
        let force_rule = Rule1D { subrules: vec![sub_force] };
        let force_plan = Rule1DPlan::new(&force_rule, CellType::inactive());
        let mut rng2 = SmallRng::seed_from_u64(7);
        let out2 = Grid1D::next_type_interior(&cells, &force_rule, &force_plan, CellType::inactive(), 1, Some(&mut rng2));
        assert_eq!(out2, CellType::inactive());
    }
}

