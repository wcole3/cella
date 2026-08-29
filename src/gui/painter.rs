//! Turning the grid into rectangles on screen.
//!
//! Painting a cell-per-rectangle would swamp the renderer, so a row is walked
//! left to right and neighbouring cells of the same colour are merged into one
//! wide rectangle. Cellular automata are strongly spatially correlated, so this
//! usually collapses hundreds of rectangles per row down to a handful.

use super::app::{CellaApp, Dim};
use cella_lib::CellType;
use egui::{Color32, Shape};
use lasso2::Spur;

/// Accumulates grid cells into merged, same-colored rectangles for one frame.
///
/// See [`CellaApp::paint_grid_viewport`] for why the merging matters.
pub(in crate::gui) struct RowPainter<'a, F: Fn(CellType) -> Color32> {
    shapes: &'a mut Vec<Shape>,
    /// Linear-probed color memo. Scenarios have a handful of types, so a scan
    /// beats hashing and it avoids re-running the fallback FNV hash per cell.
    color_cache: Vec<(Spur, Color32)>,
    /// Maps a cell type to its color; memoized through `color_cache`.
    resolve: F,
    /// Top-left corner of the full (unclipped) grid in screen coordinates.
    origin: egui::Pos2,
    /// Pixels per cell.
    scale: f32,
    /// Background color; runs of this color are left unpainted.
    bg: Color32,
}

impl<F: Fn(CellType) -> Color32> RowPainter<'_, F> {
    #[inline]
    pub(in crate::gui) fn color_of(&mut self, ty: CellType) -> Color32 {
        if let Some(&(_, c)) = self.color_cache.iter().find(|(s, _)| *s == ty.0) {
            return c;
        }
        let c = (self.resolve)(ty);
        self.color_cache.push((ty.0, c));
        c
    }

    /// Emit merged runs of same-colored cells for the cells `xs` of grid row `row_y`.
    pub(in crate::gui) fn emit_row(
        &mut self,
        row_y: usize,
        xs: std::ops::Range<usize>,
        cell_at: impl Fn(usize) -> CellType,
    ) {
        if xs.is_empty() {
            return;
        }
        let (x_start, x_end) = (xs.start, xs.end);
        let y = self.origin.y + row_y as f32 * self.scale;
        let mut run_start = x_start;
        let mut run_color = self.color_of(cell_at(x_start));
        for x in (x_start + 1)..=x_end {
            // At `x_end` the sentinel forces the final run to be flushed.
            let col = if x < x_end {
                self.color_of(cell_at(x))
            } else {
                run_color
            };
            if x == x_end || col != run_color {
                if run_color != self.bg {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(self.origin.x + run_start as f32 * self.scale, y),
                        egui::vec2((x - run_start) as f32 * self.scale, self.scale),
                    );
                    self.shapes.push(Shape::rect_filled(rect, 0.0, run_color));
                }
                run_start = x;
                run_color = col;
            }
        }
    }
}

impl CellaApp {
    /// Paint the grid directly using egui's Painter API with viewport culling.
    ///
    /// Only cells visible in the current scroll viewport are drawn, which
    /// eliminates the GPU texture-size limit that the old single-texture
    /// approach hit on large grids and dramatically improves performance
    /// because off-screen cells are skipped entirely.
    ///
    /// Within a visible row, horizontally adjacent cells of the same color are
    /// merged into a single rectangle. Cellular automata are highly spatially
    /// correlated, so this typically collapses hundreds of quads per row down to
    /// a handful and is the difference between the tessellator being the
    /// bottleneck and it being free.
    pub(in crate::gui) fn paint_grid_viewport(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<egui::Response> {
        // Determine logical grid dimensions in cells
        let (grid_w, grid_h) = match self.scenario.dim {
            Some(Dim::D1) => {
                let g = self.scenario.d1.as_ref()?;
                let total_rows = self.view.history_1d.len() + 1;
                let visible_rows = total_rows.max(self.view.min_view_rows_1d.max(1));
                (g.width.max(1), visible_rows)
            }
            Some(Dim::D2) => {
                let g = self.scenario.d2.as_ref()?;
                (g.width.max(1), g.height.max(1))
            }
            None => return None,
        };

        let scale = self.view.scale.max(1) as f32;
        let total_size = egui::vec2(grid_w as f32 * scale, grid_h as f32 * scale);

        // Allocate space for the full grid so the scroll area knows the content size
        let (response, painter) = ui.allocate_painter(total_size, egui::Sense::click_and_drag());
        let full_rect = response.rect;

        // Determine visible region (clip rect intersected with allocated rect)
        let clip = ui.clip_rect();
        let visible = full_rect.intersect(clip);
        if visible.width() <= 0.0 || visible.height() <= 0.0 {
            return Some(response);
        }

        // Convert visible pixel range to cell range (with one cell margin for partial visibility)
        let cell_x_start = ((visible.min.x - full_rect.min.x) / scale).floor().max(0.0) as usize;
        let cell_y_start = ((visible.min.y - full_rect.min.y) / scale).floor().max(0.0) as usize;
        let cell_x_end = ((visible.max.x - full_rect.min.x) / scale)
            .ceil()
            .min(grid_w as f32) as usize;
        let cell_y_end = ((visible.max.y - full_rect.min.y) / scale)
            .ceil()
            .min(grid_h as f32) as usize;

        // Reuse last frame's shape allocation instead of reallocating every frame.
        // Taking it out of `self` also releases the mutable borrow, so the rest of
        // this function can read `self` immutably while filling the buffer.
        let mut shapes = std::mem::take(&mut self.view.shape_buf);
        shapes.clear();
        let bg = self.inactive_color();

        // Fill visible area with inactive background
        shapes.push(Shape::rect_filled(visible, 0.0, bg));

        let mut rows = RowPainter {
            shapes: &mut shapes,
            color_cache: Vec::new(),
            resolve: |ty| self.color_of(&ty),
            origin: full_rect.min,
            scale,
            bg,
        };

        // Draw only visible cells
        match self.scenario.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.scenario.d1 {
                    let history_len = self.view.history_1d.len();
                    // History rows
                    for row_i in cell_y_start..cell_y_end.min(history_len) {
                        let row = &self.view.history_1d[row_i];
                        let x_end = cell_x_end.min(row.len().min(g.width));
                        rows.emit_row(row_i, cell_x_start..x_end, |x| row[x]);
                    }
                    // Current row at y = history_len
                    if cell_y_end > history_len && cell_y_start <= history_len {
                        let x_end = cell_x_end.min(g.width);
                        rows.emit_row(history_len, cell_x_start..x_end, |x| g.cell_type(x));
                    }
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.scenario.d2 {
                    let w = g.width;
                    let x_end = cell_x_end.min(w);
                    for y in cell_y_start..cell_y_end.min(g.height) {
                        rows.emit_row(y, cell_x_start..x_end, |x| g.cell_type(y * w + x));
                    }
                }
            }
            None => {}
        }

        // Grid lines (only for visible cells; skip when scale < 3 as lines would dominate)
        if self.view.show_grid_lines && scale >= 3.0 {
            let stroke = egui::Stroke::new(1.0, self.view.grid_line_color);
            // Vertical lines
            for cx in cell_x_start..=cell_x_end.min(grid_w) {
                let px = full_rect.min.x + cx as f32 * scale;
                shapes.push(Shape::line_segment(
                    [egui::pos2(px, visible.min.y), egui::pos2(px, visible.max.y)],
                    stroke,
                ));
            }
            // Horizontal lines
            for cy in cell_y_start..=cell_y_end.min(grid_h) {
                let py = full_rect.min.y + cy as f32 * scale;
                shapes.push(Shape::line_segment(
                    [egui::pos2(visible.min.x, py), egui::pos2(visible.max.x, py)],
                    stroke,
                ));
            }
        }

        // One batched hand-off to the painter instead of a lock per shape.
        painter.extend(shapes.drain(..));
        self.view.shape_buf = shapes;

        Some(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::render::palette_index_for;

    const RED: Color32 = Color32::RED;
    const BLUE: Color32 = Color32::BLUE;
    const GREEN: Color32 = Color32::GREEN;
    const BG: Color32 = Color32::BLACK;

    /// Run `emit_row` over `cells` and return the (x, width) of each emitted rect
    /// in cell units, so expectations read in grid coordinates rather than pixels.
    fn runs_for(cells: &[&str], xs: std::ops::Range<usize>) -> Vec<(usize, usize)> {
        const SCALE: f32 = 4.0;
        let types: Vec<CellType> = cells.iter().map(|s| CellType::from(*s)).collect();
        let mut shapes = Vec::new();
        let mut painter = RowPainter {
            shapes: &mut shapes,
            color_cache: Vec::new(),
            resolve: |ty: CellType| match ty.as_str() {
                "R" => RED,
                "B" => BLUE,
                _ => BG,
            },
            origin: egui::pos2(0.0, 0.0),
            scale: SCALE,
            bg: BG,
        };
        painter.emit_row(0, xs, |x| types[x]);
        shapes
            .iter()
            .map(|s| match s {
                Shape::Rect(r) => (
                    (r.rect.min.x / SCALE).round() as usize,
                    (r.rect.width() / SCALE).round() as usize,
                ),
                other => panic!("expected a rect, got {other:?}"),
            })
            .collect()
    }

    /// Build the synthetic row used by `paint_bench`: three foreground types
    /// (`A`, `B`, `C`) plus background, laid out as one long stretch, three
    /// single-cell islands, and background gaps — a stand-in for the strongly
    /// spatially correlated rows a real cellular-automaton grid produces.
    /// Repeating this 200-cell unit 10 times gives a 2000-cell row.
    ///
    /// Returns the row and the number of rectangles `emit_row` should paint
    /// for it, computed from the same segment list that builds the row
    /// (background segments are never painted, so only the other three count
    /// per unit).
    fn synthetic_paint_row() -> (Vec<CellType>, usize) {
        const UNITS: usize = 10;
        // (type tag, run length); "_" is background and is never painted.
        const SEGMENTS: [(&str, usize); 9] = [
            ("_", 80),
            ("A", 60),
            ("_", 10),
            ("B", 1),
            ("_", 10),
            ("B", 1),
            ("_", 10),
            ("C", 1),
            ("_", 27),
        ];

        let mut row = Vec::with_capacity(2000);
        let mut expected_rects = 0usize;
        for _ in 0..UNITS {
            for &(tag, len) in &SEGMENTS {
                if tag != "_" {
                    expected_rects += 1;
                }
                row.extend(std::iter::repeat_n(CellType::from(tag), len));
            }
        }
        assert_eq!(row.len(), 2000, "synthetic row layout must total 2000 cells");
        (row, expected_rects)
    }

    /// Timing baseline for `emit_row` (Phase 2, §2.4 / §8 E10 in
    /// `docs/performance.md`). Not a correctness test on its own — the
    /// rect-count assertion below is what guards behaviour; the printed
    /// number is the "before" figure later phases compare against.
    ///
    /// Run with:
    /// `cargo test --release --package cella --bin cella -- --ignored paint_bench --nocapture`
    #[test]
    #[ignore]
    fn paint_bench() {
        const ROWS_PER_FRAME: usize = 900;
        const WARMUP_FRAMES: usize = 2;
        const TIMED_FRAMES: usize = 20;

        let (row, expected_rects_per_row) = synthetic_paint_row();
        let expected_rects_per_frame = expected_rects_per_row * ROWS_PER_FRAME;

        let mut shapes = Vec::new();
        let mut min_ms = f64::INFINITY;

        for frame in 0..(WARMUP_FRAMES + TIMED_FRAMES) {
            // Cleared every frame so the buffer does not grow, matching how
            // `paint_grid_viewport` reuses `self.view.shape_buf`.
            shapes.clear();
            let start = std::time::Instant::now();
            {
                let mut painter = RowPainter {
                    shapes: &mut shapes,
                    color_cache: Vec::new(),
                    resolve: |ty: CellType| match ty.as_str() {
                        "A" => RED,
                        "B" => BLUE,
                        "C" => GREEN,
                        _ => BG,
                    },
                    origin: egui::pos2(0.0, 0.0),
                    scale: 1.0,
                    bg: BG,
                };
                for row_y in 0..ROWS_PER_FRAME {
                    painter.emit_row(row_y, 0..row.len(), |x| row[x]);
                }
            }
            if frame >= WARMUP_FRAMES {
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                min_ms = min_ms.min(elapsed_ms);
            }
            assert_eq!(
                shapes.len(),
                expected_rects_per_frame,
                "frame {frame}: emitted rect count drifted from the synthetic row's expected count"
            );
        }

        println!("paint_bench: {min_ms:.3} ms/frame (min of {TIMED_FRAMES})");
    }

    #[test]
    fn adjacent_same_color_cells_merge_into_one_rect() {
        assert_eq!(runs_for(&["R", "R", "R", "R"], 0..4), vec![(0, 4)]);
    }

    #[test]
    fn background_runs_are_not_painted() {
        // Only the two "R" cells at x=1..3 should produce geometry.
        assert_eq!(runs_for(&["_", "R", "R", "_"], 0..4), vec![(1, 2)]);
    }

    #[test]
    fn distinct_colors_split_into_separate_rects() {
        assert_eq!(
            runs_for(&["R", "R", "B", "R"], 0..4),
            vec![(0, 2), (2, 1), (3, 1)]
        );
    }

    #[test]
    fn trailing_run_is_flushed() {
        // Regression guard: the final run must be emitted when the row ends
        // mid-run rather than on a color change.
        assert_eq!(runs_for(&["B", "R", "R"], 0..3), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn only_the_requested_x_window_is_painted() {
        assert_eq!(runs_for(&["R", "R", "R", "R"], 1..3), vec![(1, 2)]);
    }

    #[test]
    fn empty_range_emits_nothing() {
        assert_eq!(runs_for(&["R", "R"], 1..1), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn all_background_row_emits_nothing() {
        assert_eq!(
            runs_for(&["_", "_", "_"], 0..3),
            Vec::<(usize, usize)>::new()
        );
    }
}
