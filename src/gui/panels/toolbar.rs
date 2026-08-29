//! The top toolbar: play/pause, stepping, zoom, export, and panel toggles.

use crate::gui::app::CellaApp;
use egui::Context;

impl CellaApp {
    /// Build the top toolbar: play/pause, step, run-to, scale, export/save/reset.
    pub(in crate::gui) fn ui_top_controls(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        ui.horizontal(|ui| {
            if ui
                .button(if self.playback.playing {
                    "Pause"
                } else {
                    "Play"
                })
                .clicked()
            {
                self.toggle_play();
            }
            if ui.button("Step").clicked() {
                self.step_once();
                self.set_status(format!("Stepped to {}", self.current_step()));
            }
            ui.add(
                egui::DragValue::new(&mut self.playback.refresh_ms)
                    .range(10..=2000)
                    .suffix(" ms"),
            );
            ui.label("Refresh");
            ui.separator();
            ui.add(
                egui::DragValue::new(&mut self.playback.run_to_steps)
                    .range(1..=1_000_000)
                    .suffix(" steps"),
            );
            if ui.button("Run to +N").clicked() {
                self.start_run_to();
            }
            ui.separator();
            ui.add(
                egui::DragValue::new(&mut self.view.scale)
                    .range(1..=64)
                    .suffix(" px"),
            );
            ui.label("Scale");
            ui.separator();
            let exporting = self.export.join.is_some();
            let export_btn = ui.add_enabled(!exporting, egui::Button::new("Export GIF..."));
            if export_btn.clicked() {
                self.export_gif_dialog();
            }
            if exporting {
                ui.label("Exporting...");
            }
            if ui.button("Save Final State").clicked() {
                self.save_final_state();
            }
            if ui.button("Reset").clicked() {
                self.reset_to_initial();
            }
            ui.separator();
            let toggle = if self.editor.visible {
                "Hide Rule Editor"
            } else {
                "Show Rule Editor"
            };
            if ui.button(toggle).clicked() {
                self.editor.visible = !self.editor.visible;
            }
        });
    }
}
