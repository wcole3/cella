//! 2D grid implementation.

use std::io::Error;
use crate::chunking::{split_chunks, OutChunk};
use crate::rules::{apply_counts, Rule2D, Rule2DPlan, TypeCounter};
use crate::threads::{chunks_for_work, pool};
use crate::types::{CellState, CellType};
use lasso2::Spur;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
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
            rule: Rule2D,
            counts_current: std::collections::HashMap<Spur, u64>,
            peak_counts: std::collections::HashMap<Spur, u64>,
            inactive: CellType,
            #[serde(default)]
            model: Option<Box<dyn crate::external::ExternalModel>>,
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
            let cnt = counts_current.entry(c.0).or_insert(0);
            *cnt += 1;
            if *cnt > dominant_type.1 {
                dominant_type = (*c, *cnt);
            }
        }
        let peak_counts = counts_current.clone();
        let inactive = CellType::inactive();
        Self { width, height, history_limit, ages, cells: initial,
            next_cells, history_data, history_heads, history_counts,
            step: 0, rule, counts_current, peak_counts, inactive, dominant_type: dominant_type.0,
            model: None }
    }

    /// Transition cell `idx` to `new_type`.
    pub fn transition_state_and_buffer(&mut self, idx: usize, new_type: &CellType) -> Option<Error> {
        if idx >= self.width * self.height {
            Some(Error::new(std::io::ErrorKind::InvalidInput, "Index out of bounds"))
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
    fn neighbor(cells: &[CellType], inactive: CellType, width: usize, height: usize, x: isize, y: isize) -> CellType {
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
        cells: &[CellType], rule: &Rule2D, plan: &Rule2DPlan, inactive: CellType,
        idx: usize, mut rng: Option<&mut SmallRng>,
    ) -> CellType {
        let current_type = cells[idx];
        for (i, sr) in rule.subrules.iter().enumerate() {
            if current_type != sr.current_type { continue; }
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
                    if cells[idx.wrapping_add_signed(off)] == crit { neighbors += 1; }
                    if early && neighbors >= target { break; }
                }
            }
            if sr.eval_condition(neighbors) {
                if let Some(r) = sr.randomness {
                    if let Some(rng) = rng.as_deref_mut() {
                        if rng.r#gen::<f64>() < r { continue; }
                    }
                }
                return sr.output_type;
            }
        }
        inactive
    }

    /// Evaluate subrules for a cell near a border, treating out-of-bounds
    /// neighbors as `inactive`.
    ///
    #[inline]
    fn next_type_edge(
        cells: &[CellType], rule: &Rule2D, inactive: CellType,
        width: usize, height: usize, x: isize, y: isize, idx: usize,
        mut rng: Option<&mut SmallRng>,
    ) -> CellType {
        let current_type = cells[idx];
        for sr in &rule.subrules {
            if current_type != sr.current_type { continue; }
            let crit = sr.criteria_type;
            let early = sr.early_exit;
            let target = sr.count;
            let mut neighbors = 0u32;
            if !(early && target == 0) {
                for off in &sr.offsets {
                    if Self::neighbor(cells, inactive, width, height, x + off.0 as isize, y + off.1 as isize) == crit {
                        neighbors += 1;
                    }
                    if early && neighbors >= target { break; }
                }
            }
            if sr.eval_condition(neighbors) {
                if let Some(r) = sr.randomness {
                    if let Some(rng) = rng.as_deref_mut() {
                        if rng.r#gen::<f64>() < r { continue; }
                    }
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
    fn step_chunk(
        cells: &[CellType], out: &mut OutChunk<'_>, history_limit: usize,
        rule: &Rule2D, plan: &Rule2DPlan, inactive: CellType, dt: CellType,
        width: usize, height: usize,
    ) -> TypeCounter {
        let mut count_map = TypeCounter::new();
        // Only pay for RNG setup when a subrule actually draws from it.
        let mut rng = if plan.needs_rng { Some(SmallRng::from_entropy()) } else { None };
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
                Self::next_type_interior(cells, rule, plan, inactive, idx, rng.as_mut())
            } else {
                Self::next_type_edge(cells, rule, inactive, width, height,
                    x as isize, y as isize, idx, rng.as_mut())
            };
            next_cells[local] = new_type;
            if history_limit > 0 {
                let base = local * history_limit;
                let h = history_heads[local] as usize;
                history_data[base + h] = cur;
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
        let tail_mask = if w % 64 != 0 { (1u64 << (w % 64)) - 1 } else { !0u64 };
        for y in 0..h {
            let cur_row = &bits[y * wpr..(y + 1) * wpr];
            let up: &[u64] = if y > 0 { &bits[(y - 1) * wpr..y * wpr] } else { &zero_row };
            let dn: &[u64] = if y + 1 < h { &bits[(y + 1) * wpr..(y + 2) * wpr] } else { &zero_row };
            for k in 0..wpr {
                let shl = |r: &[u64]| (r[k] << 1) | if k > 0 { r[k - 1] >> 63 } else { 0 };
                let shr = |r: &[u64]| (r[k] >> 1) | if k + 1 < wpr { r[k + 1] << 63 } else { 0 };
                // In neighborhood_offsets(Moore, 1) order — sorted (dx, dy):
                // (-1,-1) (-1,0) (-1,1) (0,-1) (0,1) (1,-1) (1,0) (1,1).
                // dx = -1 means "west neighbor": bit j reads bit j-1 => shl.
                let planes = [
                    shl(up), shl(cur_row), shl(dn),
                    up[k], dn[k],
                    shr(up), shr(cur_row), shr(dn),
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
        if total > 0 {
            if let Some(pt) = plan.packed {
                if let Some(count_map) = self.step_packed(pt) {
                    std::mem::swap(&mut self.cells, &mut self.next_cells);
                    apply_counts(&mut self.counts_current, &mut self.peak_counts,
                        &mut self.dominant_type, total as u64, &count_map);
                    self.step = self.step.saturating_add(1);
                    return;
                }
            }
        }

        let nchunks = chunks_for_work(total.saturating_mul(plan.work_per_cell));
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
            Self::step_chunk(cells, &mut out, hl, rule, &plan, inactive, dt, width, height)
        } else {
            let chunk = total.div_ceil(nchunks);
            let mut chunks = split_chunks(
                &mut self.next_cells, &mut self.ages,
                &mut self.history_data, &mut self.history_heads, &mut self.history_counts,
                hl, chunk,
            );
            // Persistent pool: no thread spawn/join per step.
            pool(nchunks).install(|| {
                chunks.par_iter_mut()
                    .map(|c| Self::step_chunk(cells, c, hl, rule, &plan, inactive, dt, width, height))
                    .reduce(TypeCounter::new, |mut a, b| { a.merge(&b); a })
            })
        };

        std::mem::swap(&mut self.cells, &mut self.next_cells);
        apply_counts(&mut self.counts_current, &mut self.peak_counts,
            &mut self.dominant_type, total as u64, &count_map);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{CountOp, Neighborhood2D, Rule2DSubrule};
    use crate::threads::{clear_min_work_per_chunk_override, clear_thread_override, set_min_work_per_chunk_override, set_thread_override};
    use serde_json::json;

    #[test]
    fn deserialize_and_debug_paths_work() {
        let inactive = CellType::inactive();
        let a = CellType::from("A");
        let counts: std::collections::HashMap<Spur, u64> = [(a.0, 3u64), (inactive.0, 1u64)].into_iter().collect();
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
        assert!(g.transition_state_and_buffer(0, &CellType::inactive()).is_none());
        assert!(g.history_data.is_empty());
    }

    #[test]
    fn rng_none_paths_are_exercised_for_interior_and_edge() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let sr = Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, Some(0.5), None);
        let rule = Rule2D { subrules: vec![sr] };

        let cells = vec![a; 9];
        let plan = Rule2DPlan::new(&rule, 3);
        let interior = Grid2D::next_type_interior(&cells, &rule, &plan, CellType::inactive(), 4, None);
        assert_eq!(interior, b);

        let edge = Grid2D::next_type_edge(&cells, &rule, CellType::inactive(), 3, 3, 0, 0, 0, None);
        assert_eq!(edge, b);
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
        let sr = Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, Some(1.0), None);
        let rule = Rule2D { subrules: vec![sr] };
        let cells = vec![a; 9];
        let plan = Rule2DPlan::new(&rule, 3);
        let mut rng1 = SmallRng::seed_from_u64(7);
        let mut rng2 = SmallRng::seed_from_u64(9);
        let interior = Grid2D::next_type_interior(&cells, &rule, &plan, CellType::inactive(), 4, Some(&mut rng1));
        let edge = Grid2D::next_type_edge(&cells, &rule, CellType::inactive(), 3, 3, 0, 0, 0, Some(&mut rng2));
        assert_eq!(interior, CellType::inactive());
        assert_eq!(edge, CellType::inactive());

        // Force deterministic continue in both paths via r>1 (internal-path coverage).
        let sr_force = Rule2DSubrule::new(a, a, 0, CountOp::Gt, 1, Neighborhood2D::Moore, b, Some(2.0), None);
        let rule_force = Rule2D { subrules: vec![sr_force] };
        let plan_force = Rule2DPlan::new(&rule_force, 3);
        let mut rng3 = SmallRng::seed_from_u64(11);
        let mut rng4 = SmallRng::seed_from_u64(13);
        let interior2 = Grid2D::next_type_interior(&cells, &rule_force, &plan_force, CellType::inactive(), 4, Some(&mut rng3));
        let edge2 = Grid2D::next_type_edge(&cells, &rule_force, CellType::inactive(), 3, 3, 0, 0, 0, Some(&mut rng4));
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
}

