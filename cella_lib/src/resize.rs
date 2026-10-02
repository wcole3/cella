//! Moving per-cell arrays between grid sizes.
//!
//! A resize keeps the top-left corner fixed: cell (x, y) of the old grid is
//! cell (x, y) of the new grid wherever both exist. Cells only the old grid
//! had are dropped (cropped); cells only the new grid has are filled in
//! (padded). These helpers work on plain row-major slices so the grids and
//! the models can share them.

use crate::external::ModelError;
use crate::types::CellType;
use lasso2::Spur;
use std::collections::HashMap;

/// Why a grid refused to change size. In every case the grid is unchanged.
#[derive(Debug, thiserror::Error)]
pub enum ResizeError {
    /// Width or height was 0.
    #[error("grid size must be at least 1x1, got {width}x{height}")]
    ZeroSize { width: usize, height: usize },
    /// `width * height` (or that times the history length) overflows `usize`.
    #[error("grid size {width}x{height} is too large")]
    TooLarge { width: usize, height: usize },
    /// The attached model could not fit itself to the new size.
    #[error("model rejected the new size: {0}")]
    Model(#[from] ModelError),
}

/// Check a requested size and return its cell count (`width * height`).
///
/// The history buffer holds `history_limit` entries per cell (at least 1 is
/// assumed here, to be safe), so `cells * history_limit` must fit in `usize`
/// too, otherwise [`ResizeError::TooLarge`] is returned.
pub(crate) fn checked_cells(
    width: usize,
    height: usize,
    history_limit: usize,
) -> Result<usize, ResizeError> {
    if width == 0 || height == 0 {
        return Err(ResizeError::ZeroSize { width, height });
    }
    let n = width
        .checked_mul(height)
        .ok_or(ResizeError::TooLarge { width, height })?;
    n.checked_mul(history_limit.max(1))
        .ok_or(ResizeError::TooLarge { width, height })?;
    Ok(n)
}

/// Copy a row-major `old.0 x old.1` array into a new `new.0 x new.1` array,
/// anchored top-left. Every cell owns `block` consecutive entries (1 for a
/// plain per-cell array, `history_limit` for the flat history buffer).
/// Cells in the overlap keep their whole block; new cells get `fill`.
pub fn remap_blocks<T: Copy>(
    src: &[T],
    block: usize,
    old: (usize, usize),
    new: (usize, usize),
    fill: T,
) -> Vec<T> {
    let (old_w, old_h) = old;
    let (new_w, new_h) = new;
    debug_assert_eq!(src.len(), old_w * old_h * block);
    let mut out = vec![fill; new_w * new_h * block];
    let row = old_w.min(new_w) * block;
    for y in 0..old_h.min(new_h) {
        let from = y * old_w * block;
        let to = y * new_w * block;
        out[to..to + row].copy_from_slice(&src[from..from + row]);
    }
    out
}

/// Like [`remap_blocks`] with one entry per cell, but a new cell copies the
/// nearest old cell instead of a fixed value ("edge replication"): x is
/// clamped to the old width and y to the old height. Used for terrain-like
/// layers, so a pad continues the edge instead of inventing a cliff.
/// `src` must be non-empty, with `old` at least 1x1.
pub fn remap_edge<T: Copy>(src: &[T], old: (usize, usize), new: (usize, usize)) -> Vec<T> {
    let (old_w, old_h) = old;
    let (new_w, new_h) = new;
    debug_assert_eq!(src.len(), old_w * old_h);
    let mut out = Vec::with_capacity(new_w * new_h);
    for y in 0..new_h {
        let sy = y.min(old_h - 1);
        for x in 0..new_w {
            let sx = x.min(old_w - 1);
            out.push(src[sy * old_w + sx]);
        }
    }
    out
}

/// Recount population per type from `cells`, and raise `peak_counts` wherever
/// a new count is higher (a peak that happened stays a peak).
///
/// `inactive` is the grid's background type. It is the dominant type reported
/// for an empty `cells` slice (a normal grid never is, since size >= 1x1).
///
/// Shared by [`crate::Grid1D::resize`] and [`crate::Grid2D::resize`] so the
/// bookkeeping after a resize is written once. Returns the freshly computed
/// `counts_current` map and the new dominant type; the caller stores both.
/// `peak_counts` is updated in place rather than returned, since it is
/// carried over from before the resize, not rebuilt from scratch.
pub(crate) fn recount(
    cells: &[CellType],
    inactive: CellType,
    peak_counts: &mut HashMap<Spur, u64>,
) -> (HashMap<Spur, u64>, CellType) {
    let mut counts: HashMap<Spur, u64> = HashMap::new();
    let mut dominant: (CellType, u64) = (inactive, 0);
    for c in cells {
        let cnt = counts.entry(c.0).or_insert(0);
        *cnt += 1;
        if *cnt > dominant.1 {
            dominant = (*c, *cnt);
        }
    }
    for (k, v) in &counts {
        let peak = peak_counts.entry(*k).or_insert(0);
        if *v > *peak {
            *peak = *v;
        }
    }
    (counts, dominant.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 3x2 grid:  1 2 3
    //            4 5 6
    const SRC: [i32; 6] = [1, 2, 3, 4, 5, 6];

    #[test]
    fn remap_blocks_grows_with_fill_and_keeps_the_overlap() {
        let out = remap_blocks(&SRC, 1, (3, 2), (4, 3), 0);
        assert_eq!(out, vec![1, 2, 3, 0, 4, 5, 6, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn remap_blocks_shrinks_by_cropping_right_and_bottom() {
        assert_eq!(remap_blocks(&SRC, 1, (3, 2), (2, 1), 0), vec![1, 2]);
    }

    #[test]
    fn remap_blocks_moves_whole_blocks() {
        // 2x1 grid, two entries per cell.
        let src = [10, 11, 20, 21];
        let out = remap_blocks(&src, 2, (2, 1), (1, 2), -1);
        assert_eq!(out, vec![10, 11, -1, -1]);
    }

    #[test]
    fn remap_edge_copies_the_nearest_edge_cell() {
        let out = remap_edge(&SRC, (3, 2), (4, 3));
        assert_eq!(out, vec![1, 2, 3, 3, 4, 5, 6, 6, 4, 5, 6, 6]);
        assert_eq!(remap_edge(&SRC, (3, 2), (1, 1)), vec![1]);
    }

    #[test]
    fn recount_computes_counts_and_dominant_and_raises_peaks_without_lowering_them() {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let inactive = CellType::inactive();
        let cells = vec![a, a, b];
        let mut peaks: HashMap<Spur, u64> = HashMap::new();
        // A stale, higher peak for `a` must survive; `b` has no prior peak.
        peaks.insert(a.0, 5);
        let (counts, dominant) = recount(&cells, inactive, &mut peaks);
        assert_eq!(counts[&a.0], 2);
        assert_eq!(counts[&b.0], 1);
        assert_eq!(dominant, a, "a is the majority type");
        assert_eq!(peaks[&a.0], 5, "an existing higher peak is not lowered");
        assert_eq!(peaks[&b.0], 1, "a fresh type's peak starts at its new count");
    }

    #[test]
    fn checked_cells_rejects_zero_and_overflow() {
        assert!(matches!(checked_cells(0, 3, 0), Err(ResizeError::ZeroSize { .. })));
        assert!(matches!(checked_cells(3, 0, 0), Err(ResizeError::ZeroSize { .. })));
        assert!(matches!(checked_cells(usize::MAX, 2, 0), Err(ResizeError::TooLarge { .. })));
        assert!(matches!(
            checked_cells(usize::MAX / 2, 1, 4),
            Err(ResizeError::TooLarge { .. })
        ));
        assert_eq!(checked_cells(4, 5, 3).unwrap(), 20);
    }
}
