//! The MAP-Elites archive gallery's arithmetic: which archive cell sits
//! where on the heat map, which elites make the thumbnail strip, and how a
//! thumbnail becomes pixels. Kept free of widgets so it can be unit-tested.

use cella_lib::CellType;
use cella_lib::explore::{ArchiveSnapshot, Thumbnail};
use egui::{Color32, ColorImage};

/// Most elites shown in the thumbnail strip.
pub(in crate::gui) const STRIP_MAX: usize = 32;
/// Side of a thumbnail on screen, in points.
pub(in crate::gui) const THUMB_SIDE: f32 = 64.0;

/// The heat map's grid: `cols` along the first descriptor, `rows` along the
/// second (one row for a 1-D archive). Each slot holds the best archive cell
/// index and its fitness among the cells that project onto it (a 3-D archive
/// collapses its third axis).
pub(in crate::gui) struct HeatGrid {
    pub cols: usize,
    pub rows: usize,
    pub slots: Vec<Option<(usize, f64)>>,
}

impl HeatGrid {
    pub(in crate::gui) fn slot(&self, col: usize, row: usize) -> Option<(usize, f64)> {
        self.slots.get(row * self.cols + col).copied().flatten()
    }
}

/// Lay a snapshot out on the first two descriptor axes.
pub(in crate::gui) fn heat_grid(snap: &ArchiveSnapshot) -> HeatGrid {
    let cols = snap.dims.first().copied().unwrap_or(1).max(1) as usize;
    let rows = snap.dims.get(1).copied().unwrap_or(1).max(1) as usize;
    // Cells beyond the second axis, per (col, row) slot.
    let rest: usize = snap
        .dims
        .iter()
        .skip(2)
        .map(|d| *d as usize)
        .product::<usize>()
        .max(1);
    let mut slots: Vec<Option<(usize, f64)>> = vec![None; cols * rows];
    for (i, cell) in snap.cells.iter().enumerate() {
        let Some(cell) = cell else {
            continue;
        };
        let flat = i / rest;
        let (col, row) = (flat / rows, flat % rows);
        if col >= cols {
            continue;
        }
        let slot = &mut slots[row * cols + col];
        if slot.is_none_or(|(_, f)| cell.fitness > f) {
            *slot = Some((i, cell.fitness));
        }
    }
    HeatGrid { cols, rows, slots }
}

/// `t` in `0..=1` for a fitness between the archive's worst and best elite;
/// 1 when they coincide.
pub(in crate::gui) fn fitness_t(snap: &ArchiveSnapshot, fitness: f64) -> f32 {
    let (lo, hi) = snap
        .cells
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), c| {
            (lo.min(c.fitness), hi.max(c.fitness))
        });
    if hi <= lo || !fitness.is_finite() {
        return 1.0;
    }
    ((fitness - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
}

/// Archive cell indices of the best `n` elites, fittest first.
pub(in crate::gui) fn top_elites(snap: &ArchiveSnapshot, n: usize) -> Vec<usize> {
    let mut idx: Vec<(usize, f64)> = snap
        .cells
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.as_ref().map(|c| (i, c.fitness)))
        .collect();
    idx.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    idx.into_iter().take(n).map(|(i, _)| i).collect()
}

/// Bin coordinates of a cell (inverse of the archive's row-major index).
pub(in crate::gui) fn coords(dims: &[u32], mut index: usize) -> Vec<u32> {
    let mut out = vec![0u32; dims.len()];
    for (slot, b) in out.iter_mut().zip(dims).rev() {
        let b = (*b as usize).max(1);
        *slot = (index % b) as u32;
        index /= b;
    }
    out
}

/// What the hover shows for a cell: each axis's bin midpoint, then fitness,
/// then the genome.
pub(in crate::gui) fn cell_tooltip(
    snap: &ArchiveSnapshot,
    index: usize,
    value_text: impl Fn(&cella_lib::ParamValue) -> String,
) -> String {
    let Some(Some(cell)) = snap.cells.get(index) else {
        return "empty".into();
    };
    let mut lines = Vec::new();
    for ((label, range), (bin, dim)) in snap
        .labels
        .iter()
        .zip(&snap.ranges)
        .zip(coords(&snap.dims, index).into_iter().zip(&snap.dims))
    {
        let width = (range[1] - range[0]) / f64::from((*dim).max(1));
        let mid = range[0] + width * (f64::from(bin) + 0.5);
        lines.push(format!("{label} ≈ {mid:.3}"));
    }
    lines.push(format!("fitness {:.4}", cell.fitness));
    for (k, v) in &cell.named {
        lines.push(format!("{k} = {}", value_text(v)));
    }
    lines.join("\n")
}

/// Pixels for a thumbnail, one per cell, in the current palette.
pub(in crate::gui) fn thumbnail_image(
    thumb: &Thumbnail,
    color_of: impl Fn(&CellType) -> Color32,
) -> ColorImage {
    let (w, h) = (thumb.width as usize, thumb.height as usize);
    let mut pixels: Vec<Color32> = thumb.cells.iter().map(&color_of).collect();
    pixels.resize(w * h, Color32::TRANSPARENT);
    ColorImage::new([w, h], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cella_lib::ParamValue;
    use cella_lib::explore::{ArchiveStats, SnapshotCell};

    fn cell(f: f64) -> Option<SnapshotCell> {
        Some(SnapshotCell {
            fitness: f,
            named: [("k".to_string(), ParamValue::Int(f as i64))]
                .into_iter()
                .collect(),
            thumbnail: Some(Thumbnail {
                width: 2,
                height: 1,
                cells: vec![CellType::from("A"), CellType::inactive()],
            }),
        })
    }

    fn snap(dims: Vec<u32>, cells: Vec<Option<SnapshotCell>>) -> ArchiveSnapshot {
        let n = cells.iter().flatten().count();
        ArchiveSnapshot {
            ranges: dims.iter().map(|_| [0.0, 1.0]).collect(),
            labels: dims
                .iter()
                .enumerate()
                .map(|(i, _)| format!("d{i}"))
                .collect(),
            dims,
            cells,
            stats: ArchiveStats {
                elites: n,
                coverage: 0.0,
                qd_score: 0.0,
                obj_max: 0.0,
                obj_mean: 0.0,
                out_of_range: 0,
            },
            generation: 0,
        }
    }

    #[test]
    fn heat_grid_follows_the_archive_index_order() {
        // 2 × 3: index = col * 3 + row.
        let s = snap(
            vec![2, 3],
            vec![cell(0.1), None, cell(0.3), None, cell(0.5), None],
        );
        let g = heat_grid(&s);
        assert_eq!((g.cols, g.rows), (2, 3));
        assert_eq!(g.slot(0, 0), Some((0, 0.1)));
        assert_eq!(g.slot(0, 2), Some((2, 0.3)));
        assert_eq!(g.slot(1, 1), Some((4, 0.5)));
        assert_eq!(g.slot(1, 0), None);
        assert_eq!(g.slot(5, 5), None);
        // 1-D archive → one row.
        let s1 = snap(vec![4], vec![None, cell(0.2), None, cell(0.9)]);
        let g1 = heat_grid(&s1);
        assert_eq!((g1.cols, g1.rows), (4, 1));
        assert_eq!(g1.slot(3, 0), Some((3, 0.9)));
        // 3-D archive collapses the third axis onto the best cell.
        let s3 = snap(vec![1, 1, 3], vec![cell(0.2), cell(0.8), cell(0.5)]);
        let g3 = heat_grid(&s3);
        assert_eq!(g3.slot(0, 0), Some((1, 0.8)));
        assert_eq!(coords(&[2, 3], 4), vec![1, 1]);
        assert_eq!(coords(&[1, 1, 3], 2), vec![0, 0, 2]);
    }

    #[test]
    fn fitness_scale_top_elites_and_tooltips() {
        let s = snap(vec![3], vec![cell(0.2), cell(0.8), cell(0.5)]);
        assert_eq!(fitness_t(&s, 0.2), 0.0);
        assert_eq!(fitness_t(&s, 0.8), 1.0);
        assert!((fitness_t(&s, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(fitness_t(&s, f64::NAN), 1.0);
        let flat = snap(vec![2], vec![cell(0.4), cell(0.4)]);
        assert_eq!(fitness_t(&flat, 0.4), 1.0);
        assert_eq!(top_elites(&s, 2), vec![1, 2]);
        assert_eq!(top_elites(&s, 10), vec![1, 2, 0]);
        assert!(top_elites(&snap(vec![2], vec![None, None]), 5).is_empty());
        let tip = cell_tooltip(&s, 1, |v| format!("{v:?}"));
        assert!(tip.starts_with("d0 ≈ 0.500"), "{tip}");
        assert!(
            tip.contains("fitness 0.8000") && tip.contains("k = Int(0)"),
            "{tip}"
        );
        assert_eq!(
            cell_tooltip(&snap(vec![1], vec![None]), 0, |_| String::new()),
            "empty"
        );
        assert_eq!(cell_tooltip(&s, 99, |_| String::new()), "empty");
    }

    #[test]
    fn thumbnail_pixels_use_the_palette_and_pad_short_data() {
        let t = Thumbnail {
            width: 3,
            height: 2,
            cells: vec![CellType::from("A"), CellType::inactive()],
        };
        let img = thumbnail_image(&t, |c| {
            if *c == CellType::inactive() {
                Color32::BLACK
            } else {
                Color32::RED
            }
        });
        assert_eq!(img.size, [3, 2]);
        assert_eq!(img.pixels[0], Color32::RED);
        assert_eq!(img.pixels[1], Color32::BLACK);
        assert_eq!(img.pixels[5], Color32::TRANSPARENT);
    }
}
