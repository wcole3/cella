//! Mouse and keyboard handling for the grid viewport.
//!
//! All of this used to live inline inside `eframe::App::ui`, nested about
//! sixteen levels deep. Pulling it out gives each gesture a name, and — more
//! usefully — lets the fiddly "which cell did they click on?" arithmetic become
//! a plain function that tests can call directly.

use super::app::{CellaApp, Dim, DrawMode};
use super::types::next_in_cycle;
use cella_lib::*;
use egui::Key;

/// Zoom limits, in screen pixels per cell.
const MIN_SCALE: usize = 1;
const MAX_SCALE: usize = 64;

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
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
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
    /// type. The whole drag becomes one undo entry.
    pub(in crate::gui) fn handle_paint(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if self.playback.playing || !matches!(self.edit.draw_mode, DrawMode::Paint) {
            return;
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            // Button released: close the batch so Ctrl+Z undoes the whole stroke.
            if let Some(batch) = self.edit.current_paint_batch.take()
                && !batch.is_empty()
            {
                self.edit.undo_stack.push(batch);
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
        let paint_ty = self
            .edit
            .selected_draw_type
            .unwrap_or_else(CellType::inactive);
        let Some(prev) = self.cell_type_at(idx) else {
            return;
        };
        if prev == paint_ty {
            return;
        }
        let batch = self.edit.current_paint_batch.get_or_insert_with(Vec::new);
        if !batch.iter().any(|(j, _)| *j == idx) {
            batch.push((idx, prev));
        }
        self.set_cell(idx, paint_ty, "Paint");
    }

    /// Cycle mode: click a cell to advance it to the next declared type.
    pub(in crate::gui) fn handle_cycle_click(&mut self, response: &egui::Response) {
        if !response.clicked()
            || self.playback.playing
            || !matches!(self.edit.draw_mode, DrawMode::Cycle)
        {
            return;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        let Some(dims) = self.grid_dims() else {
            return;
        };
        let Some(idx) = cell_index_at(pos, response.rect, self.view.scale, dims) else {
            return;
        };
        // Cycle through every *declared* type, not just the ones on the grid
        // right now — otherwise a state that has died out could never be
        // painted back in.
        let cycle_types = self.declared_types();
        let Some(current) = self.cell_type_at(idx) else {
            return;
        };
        let next = next_in_cycle(&cycle_types, current);
        self.edit.undo_stack.push(vec![(idx, current)]);
        self.set_cell(idx, next, "Cycle edit");
    }

    /// Ctrl+Z undoes the most recent edit batch, while paused.
    pub(in crate::gui) fn handle_hotkeys(&mut self, ui: &egui::Ui) {
        if self.playback.playing {
            return;
        }
        let undo = ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(Key::Z));
        if !undo {
            return;
        }
        let Some(batch) = self.edit.undo_stack.pop() else {
            return;
        };
        let limit = match self.grid_dims() {
            Some(GridDims::D1 { width, .. }) => width,
            Some(GridDims::D2 { width, height }) => width * height,
            None => return,
        };
        for (idx, prev) in batch {
            // Matches the original behaviour: skip out-of-range entries, but
            // stop entirely at the first entry the engine rejects.
            if idx < limit && !self.set_cell(idx, prev, "Undo") {
                break;
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
