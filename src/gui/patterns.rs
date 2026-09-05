//! Pattern stamps: small famous Life objects you can drop onto the grid.
//!
//! Each pattern is a list of `(dx, dy)` offsets from its top-left corner.
//! [`stamp_indices`] turns one into flat cell indices at a chosen spot,
//! dropping anything that would fall off the grid, so the same table works on
//! any 2D grid size.

/// A named shape as offsets from its top-left corner.
pub(in crate::gui) struct Pattern {
    pub name: &'static str,
    pub cells: &'static [(i32, i32)],
}

/// The stamps offered in the Edit tab.
pub(in crate::gui) const PATTERNS: &[Pattern] = &[
    Pattern {
        name: "Glider",
        cells: &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)],
    },
    Pattern {
        name: "Lightweight spaceship",
        cells: &[
            (1, 0),
            (4, 0),
            (0, 1),
            (0, 2),
            (4, 2),
            (0, 3),
            (1, 3),
            (2, 3),
            (3, 3),
        ],
    },
    Pattern {
        name: "R-pentomino",
        cells: &[(1, 0), (2, 0), (0, 1), (1, 1), (1, 2)],
    },
    Pattern {
        name: "Acorn",
        cells: &[(1, 0), (3, 1), (0, 2), (1, 2), (4, 2), (5, 2), (6, 2)],
    },
];

/// Flat indices of `pattern` placed with its top-left corner at `(x, y)` on a
/// `width × height` grid, clipped to the grid.
pub(in crate::gui) fn stamp_indices(
    pattern: &Pattern,
    at: (usize, usize),
    width: usize,
    height: usize,
) -> Vec<usize> {
    pattern
        .cells
        .iter()
        .filter_map(|&(dx, dy)| {
            let x = at.0 as i64 + i64::from(dx);
            let y = at.1 as i64 + i64::from(dy);
            (x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height)
                .then(|| y as usize * width + x as usize)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_glider_lands_where_asked_and_is_clipped_at_the_edge() {
        let glider = &PATTERNS[0];
        assert_eq!(glider.name, "Glider");
        let mut idx = stamp_indices(glider, (2, 3), 10, 10);
        idx.sort_unstable();
        assert_eq!(
            idx,
            vec![3 * 10 + 3, 4 * 10 + 4, 5 * 10 + 2, 5 * 10 + 3, 5 * 10 + 4]
        );
        // Two rows fall off the bottom of a 10x4 grid.
        assert_eq!(stamp_indices(glider, (2, 3), 10, 4).len(), 1);
        assert!(stamp_indices(glider, (20, 20), 10, 10).is_empty());
    }

    #[test]
    fn every_pattern_has_distinct_offsets_and_a_name() {
        for p in PATTERNS {
            assert!(!p.name.is_empty());
            let mut cells = p.cells.to_vec();
            cells.sort_unstable();
            cells.dedup();
            assert_eq!(cells.len(), p.cells.len(), "{} repeats a cell", p.name);
            assert_eq!(stamp_indices(p, (0, 0), 20, 20).len(), p.cells.len());
        }
    }
}
