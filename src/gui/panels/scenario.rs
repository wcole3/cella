//! The "Load/Select Scenario" section of the left panel: demo buttons, the
//! custom 1D builder, grid-line options, and grid resizing.

use crate::gui::app::{CellaApp, Dim};
use egui::TextEdit;

impl CellaApp {
    pub(in crate::gui) fn ui_dataset_controls(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Load/Select Scenario", |ui| {
            ui.vertical(|ui| {
                if ui.button("Load Config JSON...").clicked() { self.load_config_dialog(); }
                if ui.button("Demo: Life (2D)").clicked() { self.load_demo_life(); }
                if ui.button("Demo: 1D Rule 30").clicked() { self.load_demo_1d_rule30(); }
                if ui.button("Demo: 1D n=2").clicked() { self.load_demo_1d_n2(); }
                if ui.button("Demo: 2D three-state").clicked() { self.load_demo_2d_three_state_cycle(); }
                if ui.button("Demo: 2D straightline").clicked() { self.load_demo_2d_straightline(); }
            });
            ui.separator();
            ui.label("Custom 1D (Wolfram code + n):");
            ui.horizontal(|ui| {
                ui.label("code:");
                // text edit with a smaller area
                ui.add_sized([20.0, 20.0], TextEdit::singleline(&mut self.inputs.custom_code));
                ui.label("n:");
                ui.add(egui::DragValue::new(&mut self.inputs.custom_n).range(1..=8));
                if ui.button("Build").clicked() { self.load_demo_1d_custom_from_inputs(); }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.view.show_grid_lines, "Grid lines");
                let mut col = self.view.grid_line_color;
                if ui.color_edit_button_srgba(&mut col).changed() { self.view.grid_line_color = col; }
            });
            ui.horizontal(|ui| {
                ui.label("1D history limit:");
                ui.add(egui::DragValue::new(&mut self.view.history_limit_1d).range(1..=10_000));
            });
            ui.separator();
            ui.label("Grid size:");
            ui.horizontal(|ui| {
                ui.label("W:");
                ui.add(egui::DragValue::new(&mut self.inputs.grid_width).range(1..=2000).speed(1));
                if matches!(self.scenario.dim, Some(Dim::D2)) {
                    ui.label("H:");
                    ui.add(egui::DragValue::new(&mut self.inputs.grid_height).range(1..=2000).speed(1));
                }
                if ui.button("Resize").on_hover_text("Rebuild the grid with the specified dimensions. Existing cells are preserved where they overlap; new cells are Inactive.").clicked() {
                    self.resize_grid();
                }
            });
            ui.separator();
            // Rule editor moved to the right panel; see right-side Rule Editor panel.
        });
    }
}
