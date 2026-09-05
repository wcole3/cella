//! Overlay layers drawn over the cells: a heat map of cell ages (fronts
//! glow) and the Explore ensemble's probability map.
//!
//! A layer is painted the way the cells are — one merged rectangle per run of
//! equal value along a row — so it costs about what the cells cost. Values
//! are first **quantised** to sixteen levels; neighbouring cells with nearly
//! the same value then share a level and merge, which is what keeps a 256×256
//! probability map cheap to draw.

use egui::{Color32, Shape};

use super::app::{CellaApp, Dim};
use super::render::{age_color, heat_color, with_opacity};

/// Which overlay a toggle refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::gui) enum Layer {
    Grid,
    Age,
    Probability,
}

/// A per-cell probability map handed over by the Explore worker.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct ProbabilityMap {
    pub width: usize,
    pub height: usize,
    /// One value in `0..=1` per cell, row-major.
    pub cells: Vec<f32>,
    /// The ensemble step the map describes.
    pub steps: u64,
}

/// Which layers are on and how strong they are.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct LayerState {
    /// Tint cells by how recently they changed.
    pub age: bool,
    /// Ages at or above this many steps are not tinted.
    pub age_cap: u32,
    /// Show the ensemble probability map when there is one.
    pub probability: bool,
    /// Opacity of both layers, `0..=1`.
    pub opacity: f32,
    pub probability_map: Option<ProbabilityMap>,
}

impl Default for LayerState {
    fn default() -> Self {
        LayerState {
            age: false,
            age_cap: 64,
            probability: true,
            opacity: 0.6,
            probability_map: None,
        }
    }
}

/// Number of distinct tints an overlay uses.
pub(in crate::gui) const LEVELS: u8 = 16;

/// Map `0..=1` onto `0..LEVELS`; anything outside is clamped, NaN is 0.
pub(in crate::gui) fn quantize(t: f32) -> u8 {
    if t.is_nan() {
        return 0;
    }
    (t.clamp(0.0, 1.0) * f32::from(LEVELS - 1)).round() as u8
}

/// Emit merged rectangles for row `row_y`, cells `xs`. `level_at(x)` gives a
/// cell's tint level, or `None` to leave it untouched; runs of equal level
/// become one rectangle coloured by `color_of_level`.
pub(in crate::gui) fn emit_overlay_row(
    shapes: &mut Vec<Shape>,
    row_y: usize,
    xs: std::ops::Range<usize>,
    level_at: impl Fn(usize) -> Option<u8>,
    color_of_level: impl Fn(u8) -> Color32,
    origin: egui::Pos2,
    scale: f32,
) {
    if xs.is_empty() {
        return;
    }
    let y = origin.y + row_y as f32 * scale;
    let flush = |shapes: &mut Vec<Shape>, start: usize, end: usize, level: u8| {
        let rect = egui::Rect::from_min_size(
            egui::pos2(origin.x + start as f32 * scale, y),
            egui::vec2((end - start) as f32 * scale, scale),
        );
        shapes.push(Shape::rect_filled(rect, 0.0, color_of_level(level)));
    };
    let mut run: Option<(usize, u8)> = None;
    for x in xs.clone() {
        let here = level_at(x);
        match (run, here) {
            (Some((start, lvl)), Some(l)) if l == lvl => {
                let _ = start;
            }
            (Some((start, lvl)), other) => {
                flush(shapes, start, x, lvl);
                run = other.map(|l| (x, l));
            }
            (None, Some(l)) => run = Some((x, l)),
            (None, None) => {}
        }
    }
    if let Some((start, lvl)) = run {
        flush(shapes, start, xs.end, lvl);
    }
}

impl CellaApp {
    /// Paint the enabled overlays for the visible window. Called by the
    /// viewport painter after the cells and before the grid lines.
    pub(in crate::gui) fn emit_layers(
        &self,
        shapes: &mut Vec<Shape>,
        origin: egui::Pos2,
        scale: f32,
        xs: std::ops::Range<usize>,
        ys: std::ops::Range<usize>,
    ) {
        let layers = &self.view.layers;
        let alpha = layers.opacity.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        if layers.age {
            let cap = layers.age_cap.max(1);
            let level_of_age = |age: u32| -> Option<u8> {
                if age >= cap {
                    return None;
                }
                let t = 1.0 - age as f32 / cap as f32;
                let l = quantize(t);
                (l > 0).then_some(l)
            };
            let color =
                |l: u8| with_opacity(age_color(f32::from(l) / f32::from(LEVELS - 1)), alpha);
            match self.scenario.dim {
                Some(Dim::D2) => {
                    if let Some(g) = &self.scenario.d2 {
                        let w = g.width;
                        for y in ys.clone().filter(|y| *y < g.height) {
                            let x_end = xs.end.min(w);
                            emit_overlay_row(
                                shapes,
                                y,
                                xs.start..x_end,
                                |x| level_of_age(g.cell_age(y * w + x)),
                                color,
                                origin,
                                scale,
                            );
                        }
                    }
                }
                Some(Dim::D1) => {
                    // Only the live row has ages; history rows are pictures.
                    if let Some(g) = &self.scenario.d1 {
                        let live = self.view.history_1d.len();
                        if ys.contains(&live) {
                            emit_overlay_row(
                                shapes,
                                live,
                                xs.start..xs.end.min(g.width),
                                |x| level_of_age(g.cell_age(x)),
                                color,
                                origin,
                                scale,
                            );
                        }
                    }
                }
                None => {}
            }
        }
        if layers.probability
            && let Some(map) = &layers.probability_map
            && let Some(Dim::D2) = self.scenario.dim
            && let Some(g) = &self.scenario.d2
            && map.width == g.width
            && map.height == g.height
        {
            let w = map.width;
            let color =
                |l: u8| with_opacity(heat_color(f32::from(l) / f32::from(LEVELS - 1)), alpha);
            for y in ys.filter(|y| *y < map.height) {
                emit_overlay_row(
                    shapes,
                    y,
                    xs.start..xs.end.min(w),
                    |x| {
                        let p = map.cells[y * w + x];
                        (p >= 1.0 / 32.0).then(|| quantize(p).max(1))
                    },
                    color,
                    origin,
                    scale,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_clamps_and_rounds_to_sixteen_levels() {
        assert_eq!(quantize(0.0), 0);
        assert_eq!(quantize(1.0), 15);
        assert_eq!(quantize(0.5), 8);
        assert_eq!(quantize(-3.0), 0);
        assert_eq!(quantize(7.0), 15);
        assert_eq!(quantize(f32::NAN), 0);
    }

    fn rects(shapes: &[Shape]) -> Vec<(f32, f32, u8)> {
        shapes
            .iter()
            .map(|s| match s {
                Shape::Rect(r) => (r.rect.min.x, r.rect.width(), r.fill.r()),
                _ => panic!("only rects expected"),
            })
            .collect()
    }

    #[test]
    fn overlay_rows_merge_equal_levels_and_skip_gaps() {
        let levels = [Some(3u8), Some(3), None, Some(3), Some(5), Some(5)];
        let mut shapes = Vec::new();
        emit_overlay_row(
            &mut shapes,
            0,
            0..6,
            |x| levels[x],
            |l| Color32::from_rgb(l, 0, 0),
            egui::pos2(10.0, 20.0),
            4.0,
        );
        // [0,2) level 3 | gap | [3,4) level 3 | [4,6) level 5
        assert_eq!(
            rects(&shapes),
            vec![(10.0, 8.0, 3), (22.0, 4.0, 3), (26.0, 8.0, 5)]
        );
        // Honours the window: only cells 1..4 are considered.
        let mut shapes = Vec::new();
        emit_overlay_row(
            &mut shapes,
            1,
            1..4,
            |x| levels[x],
            |l| Color32::from_rgb(l, 0, 0),
            egui::pos2(0.0, 0.0),
            2.0,
        );
        assert_eq!(rects(&shapes), vec![(2.0, 2.0, 3), (6.0, 2.0, 3)]);
        assert_eq!(
            shapes[0].visual_bounding_rect().min.y,
            2.0,
            "row 1 at scale 2"
        );
        let mut shapes = Vec::new();
        emit_overlay_row(
            &mut shapes,
            0,
            3..3,
            |_| Some(1),
            |_| Color32::RED,
            egui::pos2(0.0, 0.0),
            2.0,
        );
        assert!(shapes.is_empty());
    }

    #[test]
    fn the_layer_pass_draws_headless_for_age_and_probability() {
        use crate::gui::sim::tests::test_app;
        let mut app = test_app();
        app.load_demo_life();
        app.view.layers.age = true;
        app.view.layers.age_cap = 8;
        let (w, h) = (50, 30);
        app.view.layers.probability_map = Some(ProbabilityMap {
            width: w,
            height: h,
            cells: (0..w * h).map(|i| (i % 17) as f32 / 16.0).collect(),
            steps: 3,
        });
        app.apply_action(crate::gui::actions::Action::Step);
        let mut shapes = Vec::new();
        app.emit_layers(&mut shapes, egui::pos2(0.0, 0.0), 4.0, 0..w, 0..h);
        assert!(
            shapes.len() > h,
            "both layers emitted rectangles: {}",
            shapes.len()
        );
        // A map of the wrong size is ignored, opacity 0 draws nothing.
        app.view.layers.probability_map.as_mut().unwrap().width = 7;
        app.view.layers.age = false;
        let mut shapes = Vec::new();
        app.emit_layers(&mut shapes, egui::pos2(0.0, 0.0), 4.0, 0..w, 0..h);
        assert!(shapes.is_empty());
        app.view.layers.age = true;
        app.view.layers.opacity = 0.0;
        app.emit_layers(&mut shapes, egui::pos2(0.0, 0.0), 4.0, 0..w, 0..h);
        assert!(shapes.is_empty());
        // 1D: the live row only.
        let mut row = test_app();
        row.load_demo_1d_rule30();
        row.view.layers.age = true;
        row.apply_action(crate::gui::actions::Action::Step);
        let mut shapes = Vec::new();
        row.emit_layers(&mut shapes, egui::pos2(0.0, 0.0), 4.0, 0..201, 0..5);
        assert!(!shapes.is_empty());
        // The whole viewport paints with the layers on.
        egui::__run_test_ui(|ui| {
            app.view.layers.opacity = 0.6;
            let _ = app.paint_grid_viewport(ui);
        });
    }
}
