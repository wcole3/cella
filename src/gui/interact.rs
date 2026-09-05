//! Mouse and keyboard handling for the grid viewport.
//!
//! All of this used to live inline inside `eframe::App::ui`, nested about
//! sixteen levels deep. Pulling it out gives each gesture a name, and — more
//! usefully — lets the fiddly "which cell did they click on?" arithmetic become
//! a plain function that tests can call directly.

use super::actions::Action;
use super::app::{CellaApp, Dim, DrawMode};
use super::patterns::{PATTERNS, stamp_indices};
use super::shortcuts::shortcuts;
use cella_lib::rules::neighborhood_offsets;
use cella_lib::*;

/// Zoom limits, in screen pixels per cell.
pub(in crate::gui) const MIN_SCALE: usize = 1;
pub(in crate::gui) const MAX_SCALE: usize = 64;

/// Which cell a screen position falls on, or `None` if it is not on an
/// editable cell.
///
/// Split out from the gesture handlers because it is the one piece of this
/// module with arithmetic worth testing, and because the 1D and 2D cases used
/// to be written out separately in four different places.
///
/// - **2D**: any in-bounds cell, returned as the flat index `y * width + x`.
/// - **1D**: only the live bottom row is editable. The rows above it are the
///   space-time history — already-run steps, which cannot be edited after the
///   fact — so a click there returns `None`.
pub(in crate::gui) fn cell_index_at(
    pos: egui::Pos2,
    rect: egui::Rect,
    scale: usize,
    dims: GridDims,
) -> Option<usize> {
    if !rect.contains(pos) {
        return None;
    }
    let local = pos - rect.min;
    let scale = scale.max(1);
    let cell_x = local.x.max(0.0) as usize / scale;
    let cell_y = local.y.max(0.0) as usize / scale;
    match dims {
        GridDims::D1 {
            width,
            history_rows,
        } => {
            // The live row sits below `history_rows` rows of past states.
            (cell_y == history_rows && cell_x < width).then_some(cell_x)
        }
        GridDims::D2 { width, height } => {
            (cell_x < width && cell_y < height).then_some(cell_y * width + cell_x)
        }
    }
}

/// Largest brush range the Edit tab offers.
pub(in crate::gui) const MAX_BRUSH: u8 = 7;

/// Every cell a brush covers around `center`, clipped to the grid. Range 0 is
/// the single cell. On a 2D grid the footprint is the centre plus the
/// `shape` neighbourhood of that range — exactly the cells a rule with that
/// neighbourhood would count — so a Moore brush of range 1 is a 3×3 square,
/// Von Neumann a diamond, Knight the eight knight squares. On a 1D grid the
/// brush is a span of `±range` along the live row.
pub(in crate::gui) fn brush_indices(
    center: usize,
    range: u8,
    shape: Neighborhood2D,
    dims: GridDims,
) -> Vec<usize> {
    let r = i64::from(range.min(MAX_BRUSH));
    match dims {
        GridDims::D1 { width, .. } => {
            let c = center as i64;
            (c - r..=c + r)
                .filter(|x| *x >= 0 && (*x as usize) < width)
                .map(|x| x as usize)
                .collect()
        }
        GridDims::D2 { width, height } => {
            if width == 0 || height == 0 || center >= width * height {
                return Vec::new();
            }
            let (cx, cy) = ((center % width) as i64, (center / width) as i64);
            let mut out = vec![center];
            if r == 0 {
                return out;
            }
            for (dx, dy) in neighborhood_offsets(shape, r as i32) {
                let (x, y) = (cx + i64::from(dx), cy + i64::from(dy));
                if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
                    continue;
                }
                let idx = y as usize * width + x as usize;
                if idx != center {
                    out.push(idx);
                }
            }
            out.sort_unstable();
            out.dedup();
            out
        }
    }
}

/// Stroke a one-pixel outline around each listed cell of the viewport at
/// `rect`. On a 1D grid the cells sit on the live row below the history.
pub(in crate::gui) fn outline_cells(
    ui: &egui::Ui,
    rect: egui::Rect,
    scale: usize,
    dims: GridDims,
    cells: &[usize],
    color: egui::Color32,
) {
    let scale = scale.max(1) as f32;
    let stroke = egui::Stroke::new(1.0, color);
    for &cell in cells {
        let (x, y) = match dims {
            GridDims::D1 { history_rows, .. } => (cell as f32, history_rows as f32),
            GridDims::D2 { width, .. } => ((cell % width) as f32, (cell / width) as f32),
        };
        let min = rect.min + egui::vec2(x * scale, y * scale);
        ui.painter().rect_stroke(
            egui::Rect::from_min_size(min, egui::vec2(scale, scale)),
            0.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    }
}

/// The shape of the grid currently on screen, as `cell_index_at` needs to see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::gui) enum GridDims {
    D1 { width: usize, history_rows: usize },
    D2 { width: usize, height: usize },
}

impl CellaApp {
    /// The grid's on-screen shape, or `None` when no grid is loaded.
    pub(in crate::gui) fn grid_dims(&self) -> Option<GridDims> {
        match self.scenario.dim? {
            Dim::D1 => self.scenario.d1.as_ref().map(|g| GridDims::D1 {
                width: g.width,
                history_rows: self.view.history_1d.len(),
            }),
            Dim::D2 => self.scenario.d2.as_ref().map(|g| GridDims::D2 {
                width: g.width,
                height: g.height,
            }),
        }
    }

    /// Current type of cell `idx`, whichever dimension is loaded.
    pub(in crate::gui) fn cell_type_at(&self, idx: usize) -> Option<CellType> {
        match self.scenario.dim? {
            Dim::D1 => self.scenario.d1.as_ref().map(|g| g.cell_type(idx)),
            Dim::D2 => self.scenario.d2.as_ref().map(|g| g.cell_type(idx)),
        }
    }

    /// Overwrite one cell. `what` names the gesture, so the status message
    /// still says "Paint error" or "Cycle edit error" as it always did.
    ///
    /// Returns `false` if the engine rejected the write, which lets a
    /// multi-cell undo stop at the first failure rather than plough on.
    pub(in crate::gui) fn set_cell(&mut self, idx: usize, new_type: CellType, what: &str) -> bool {
        let err = match self.scenario.dim {
            Some(Dim::D1) => self
                .scenario
                .d1
                .as_mut()
                .and_then(|g| g.transition_state_and_buffer(idx, &new_type)),
            Some(Dim::D2) => self
                .scenario
                .d2
                .as_mut()
                .and_then(|g| g.transition_state_and_buffer(idx, &new_type)),
            None => None,
        };
        match err {
            Some(e) => {
                eprintln!("{what} error: {e}");
                self.set_status(format!("{what} error: {e}"));
                false
            }
            None => true,
        }
    }

    /// Mouse wheel over the grid zooms in and out.
    pub(in crate::gui) fn handle_zoom(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if !response.hovered() {
            return;
        }
        let dy = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::MouseWheel { delta, .. } = e {
                        Some(delta.y)
                    } else {
                        None
                    }
                })
                .sum::<f32>()
        });
        if dy > 0.0 {
            self.view.scale = (self.view.scale + 1).min(MAX_SCALE);
        } else if dy < 0.0 {
            self.view.scale = self.view.scale.saturating_sub(1).max(MIN_SCALE);
        }
    }

    /// Right-drag pans the scroll area. Left-drag is reserved for painting.
    pub(in crate::gui) fn handle_pan(&self, ui: &egui::Ui, response: &egui::Response) {
        if !response.dragged() {
            return;
        }
        let (right_down, delta) = ui.input(|i| (i.pointer.secondary_down(), i.pointer.delta()));
        if right_down && (delta.x != 0.0 || delta.y != 0.0) {
            ui.scroll_with_delta(delta);
        }
    }

    /// Paint mode: hold the left button and drag to set cells to the selected
    /// type, `brush` cells wide. Each frame of the drag pushes the cells under
    /// the brush; releasing the button closes the stroke into one undo entry.
    pub(in crate::gui) fn handle_paint(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if self.playback.playing || !matches!(self.edit.draw_mode, DrawMode::Paint) {
            return;
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            if self.edit.current_paint_batch.is_some() {
                self.push(Action::EndStroke);
            }
            return;
        }
        let Some(pos) = ui.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let Some(dims) = self.grid_dims() else {
            return;
        };
        let Some(idx) = cell_index_at(pos, response.rect, self.view.scale, dims) else {
            return;
        };
        let cells = brush_indices(idx, self.edit.brush, self.edit.brush_shape, dims);
        self.push(Action::PaintCells(cells));
    }

    /// Cycle mode: click a cell to advance it to the next declared type.
    pub(in crate::gui) fn handle_cycle_click(&mut self, response: &egui::Response) {
        if !response.clicked()
            || self.playback.playing
            || !matches!(self.edit.draw_mode, DrawMode::Cycle)
        {
            return;
        }
        if let Some(idx) = self.clicked_cell(response) {
            self.push(Action::CycleAt(idx));
        }
    }

    /// Stamp mode: click to drop the chosen pattern with its top-left corner
    /// on the clicked cell (2D only).
    pub(in crate::gui) fn handle_stamp_click(&mut self, response: &egui::Response) {
        if !response.clicked()
            || self.playback.playing
            || !matches!(self.edit.draw_mode, DrawMode::Stamp)
        {
            return;
        }
        if let Some(idx) = self.clicked_cell(response) {
            self.push(Action::StampAt(idx));
        }
    }

    /// The cell under the pointer for a click on the viewport.
    fn clicked_cell(&self, response: &egui::Response) -> Option<usize> {
        let pos = response.interact_pointer_pos()?;
        let dims = self.grid_dims()?;
        cell_index_at(pos, response.rect, self.view.scale, dims)
    }

    /// Outline the cells a Paint stroke or a Stamp would touch at the hover
    /// position, so the footprint is visible before the button goes down.
    pub(in crate::gui) fn draw_tool_ghost(&self, ui: &egui::Ui, response: &egui::Response) {
        if self.playback.playing || !response.hovered() {
            return;
        }
        let Some(pos) = response.hover_pos() else {
            return;
        };
        let Some(dims) = self.grid_dims() else {
            return;
        };
        let Some(idx) = cell_index_at(pos, response.rect, self.view.scale, dims) else {
            return;
        };
        let cells = match (self.edit.draw_mode, dims) {
            (DrawMode::Paint, _) => {
                brush_indices(idx, self.edit.brush, self.edit.brush_shape, dims)
            }
            (DrawMode::Stamp, GridDims::D2 { width, height }) => {
                match PATTERNS.get(self.edit.stamp) {
                    Some(p) => stamp_indices(p, (idx % width, idx / width), width, height),
                    None => return,
                }
            }
            _ => return,
        };
        outline_cells(
            ui,
            response.rect,
            self.view.scale,
            dims,
            &cells,
            self.chrome.theme.accent(),
        );
    }

    /// The hover inspector: a tooltip naming the cell under the mouse.
    pub(in crate::gui) fn show_hover_inspector(&self, ui: &egui::Ui, response: &egui::Response) {
        if !self.view.inspector || !response.hovered() || response.dragged() {
            return;
        }
        let Some(pos) = response.hover_pos() else {
            return;
        };
        let Some(dims) = self.grid_dims() else {
            return;
        };
        let Some(idx) = cell_index_at(pos, response.rect, self.view.scale, dims) else {
            return;
        };
        let Some(ty) = self.cell_type_at(idx) else {
            return;
        };
        let age = match self.scenario.dim {
            Some(Dim::D1) => self.scenario.d1.as_ref().map(|g| g.cell_age(idx)),
            Some(Dim::D2) => self.scenario.d2.as_ref().map(|g| g.cell_age(idx)),
            None => None,
        };
        let (x, y) = match dims {
            GridDims::D1 { .. } => (idx, 0),
            GridDims::D2 { width, .. } => (idx % width, idx / width),
        };
        let color = self.color_of(&ty);
        let _ = ui;
        response.clone().on_hover_ui_at_pointer(|ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color);
                ui.strong(ty.as_str());
            });
            ui.small(format!("x {x}  y {y}  index {idx}"));
            if let Some(a) = age {
                ui.small(format!("age {a} step{}", if a == 1 { "" } else { "s" }));
            }
        });
    }

    /// Turn key presses into actions, from the one table in
    /// [`super::shortcuts`].
    ///
    /// Two guards matter. First: nothing fires while a text field has the
    /// keyboard, so typing `30` into the Wolfram-code box or a type name with
    /// an `s` in it never steps or resets the grid. Second: editing keys
    /// (undo) are skipped while the simulation plays, but playback and view
    /// keys must work while playing — otherwise Space could never pause.
    /// `consume_shortcut` eats the press so nothing else reacts to it too.
    pub(in crate::gui) fn handle_hotkeys(&mut self, ui: &egui::Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let playing = self.playback.playing;
        for s in shortcuts() {
            if !s.while_playing && playing {
                continue;
            }
            if ui.input_mut(|i| i.consume_shortcut(&s.keys)) {
                self.push(s.action.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: egui::Rect = egui::Rect {
        min: egui::Pos2 { x: 10.0, y: 20.0 },
        max: egui::Pos2 { x: 110.0, y: 120.0 },
    };

    fn at(x: f32, y: f32) -> egui::Pos2 {
        egui::pos2(x, y)
    }

    #[test]
    fn d2_maps_a_position_to_a_flat_index() {
        let dims = GridDims::D2 {
            width: 10,
            height: 10,
        };
        // Top-left cell of the rect.
        assert_eq!(cell_index_at(at(10.0, 20.0), RECT, 10, dims), Some(0));
        // Three cells right, two down => 2 * 10 + 3.
        assert_eq!(cell_index_at(at(45.0, 45.0), RECT, 10, dims), Some(23));
    }

    #[test]
    fn positions_outside_the_rect_are_rejected() {
        let dims = GridDims::D2 {
            width: 10,
            height: 10,
        };
        assert_eq!(cell_index_at(at(9.0, 20.0), RECT, 10, dims), None);
        assert_eq!(cell_index_at(at(200.0, 200.0), RECT, 10, dims), None);
    }

    #[test]
    fn d2_rejects_cells_past_the_grid_edge() {
        // The rect is 10 cells wide at scale 10, but the grid is only 4x4,
        // so the right-hand part of the rect is empty space.
        let dims = GridDims::D2 {
            width: 4,
            height: 4,
        };
        assert_eq!(cell_index_at(at(15.0, 25.0), RECT, 10, dims), Some(0));
        assert_eq!(cell_index_at(at(95.0, 25.0), RECT, 10, dims), None);
        assert_eq!(cell_index_at(at(15.0, 105.0), RECT, 10, dims), None);
    }

    #[test]
    fn d1_only_the_live_row_is_editable() {
        // Two rows of history, so the live row is row index 2.
        let dims = GridDims::D1 {
            width: 10,
            history_rows: 2,
        };
        // Row 0 and row 1 are history: not editable.
        assert_eq!(cell_index_at(at(15.0, 25.0), RECT, 10, dims), None);
        assert_eq!(cell_index_at(at(15.0, 35.0), RECT, 10, dims), None);
        // Row 2 is live: editable, and the index is just the column.
        assert_eq!(cell_index_at(at(15.0, 45.0), RECT, 10, dims), Some(0));
        assert_eq!(cell_index_at(at(45.0, 45.0), RECT, 10, dims), Some(3));
        // Below the live row is empty space.
        assert_eq!(cell_index_at(at(15.0, 55.0), RECT, 10, dims), None);
    }

    #[test]
    fn d1_with_no_history_edits_row_zero() {
        let dims = GridDims::D1 {
            width: 5,
            history_rows: 0,
        };
        assert_eq!(cell_index_at(at(15.0, 25.0), RECT, 10, dims), Some(0));
        assert_eq!(cell_index_at(at(15.0, 35.0), RECT, 10, dims), None);
    }

    #[test]
    fn brush_indices_follow_the_neighbourhood_tables_and_clip() {
        use Neighborhood2D::*;
        let dims = GridDims::D2 {
            width: 10,
            height: 10,
        };
        assert_eq!(brush_indices(55, 0, Moore, dims), vec![55]);
        assert_eq!(brush_indices(55, 1, Moore, dims).len(), 9, "3x3 square");
        assert_eq!(brush_indices(55, 1, VonNeumann, dims).len(), 5, "plus");
        assert_eq!(brush_indices(55, 1, Langton, dims).len(), 5, "X");
        assert_eq!(brush_indices(55, 1, StraightLine, dims).len(), 5);
        assert_eq!(
            brush_indices(55, 1, Knight, dims).len(),
            9,
            "centre + 8 knight squares"
        );
        assert_eq!(brush_indices(55, 2, Moore, dims).len(), 25);
        // Sorted, no duplicates, centre always present.
        let k = brush_indices(55, 2, Knight, dims);
        assert!(k.windows(2).all(|w| w[0] < w[1]) && k.contains(&55));
        // Clipped at the corner: only in-bounds cells; range capped.
        let corner = brush_indices(0, 99, Moore, dims);
        assert_eq!(
            corner.len(),
            8 * 8,
            "range capped at {MAX_BRUSH}: 8x8 from the corner"
        );
        assert!(corner.contains(&0));
        let row = GridDims::D1 {
            width: 10,
            history_rows: 0,
        };
        assert_eq!(brush_indices(0, 2, Moore, row), vec![0, 1, 2]);
        assert_eq!(brush_indices(9, 1, Knight, row), vec![8, 9]);
        assert!(
            brush_indices(
                0,
                1,
                Moore,
                GridDims::D2 {
                    width: 0,
                    height: 0
                }
            )
            .is_empty()
        );
        assert!(
            brush_indices(200, 1, Moore, dims).is_empty(),
            "out-of-range centre"
        );
    }

    #[test]
    fn a_zero_scale_does_not_divide_by_zero() {
        let dims = GridDims::D2 {
            width: 4,
            height: 4,
        };
        // `scale` is clamped to at least 1, matching the painter's behaviour.
        assert_eq!(
            cell_index_at(at(12.0, 22.0), RECT, 0, dims),
            Some(2 * 4 + 2)
        );
    }
}
