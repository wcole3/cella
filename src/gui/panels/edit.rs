//! The Edit tab: which tool the mouse is, what it paints, and GIF export.

use crate::gui::actions::Action;
use crate::gui::app::{CellaApp, DrawMode};
use crate::gui::theme::section;
use cella_lib::{CellType, INACTIVE};
use std::sync::atomic::Ordering;

impl CellaApp {
    pub(in crate::gui) fn ui_edit_tab(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Action> = Vec::new();
        section(ui, "Tool", |ui| {
            ui.horizontal(|ui| {
                for (mode, label, help) in [
                    (
                        DrawMode::Cycle,
                        "Cycle",
                        "Click a cell to step it to the next type",
                    ),
                    (
                        DrawMode::Paint,
                        "Paint",
                        "Hold the left button and drag to paint the chosen type",
                    ),
                ] {
                    if ui
                        .selectable_label(self.edit.draw_mode == mode, label)
                        .on_hover_text(help)
                        .clicked()
                    {
                        pending.push(Action::SetDrawMode(mode));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Paint type");
                // Every declared type, not just the ones on the grid, so a
                // state that died out can be painted back in.
                let mut names: Vec<String> = self
                    .declared_types()
                    .into_iter()
                    .map(|t| t.as_str().to_string())
                    .collect();
                names.sort();
                names.sort_by_key(|a| a != INACTIVE);
                let current = self
                    .edit
                    .selected_draw_type
                    .map(|t| t.as_str().to_string())
                    .unwrap_or_else(|| INACTIVE.to_string());
                let mut sel = current.clone();
                egui::ComboBox::from_id_salt("paint_type")
                    .selected_text(sel.clone())
                    .show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(&mut sel, n.clone(), n);
                        }
                    });
                if sel != current {
                    pending.push(Action::SetDrawType(CellType::from(sel.as_str())));
                }
            });
            ui.small("Editing works while paused. Ctrl+Z undoes the last stroke.");
        });
        section(ui, "Export GIF", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.export.steps).range(1..=10_000));
                ui.label("steps");
                ui.add(egui::DragValue::new(&mut self.export.fps).range(1..=60));
                ui.label("fps");
            });
            ui.checkbox(
                &mut self.export.with_history_1d,
                "1D: stack rows into a space-time image",
            )
            .on_hover_text("Applies to 1D GIF export; height limited by the 1D history limit.");
            let exporting = self.export.join.is_some();
            if ui
                .add_enabled(!exporting, egui::Button::new("Export GIF\u{2026}"))
                .clicked()
            {
                pending.push(Action::ExportGif);
            }
            if let Some(p) = &self.export.progress {
                let done = p.load(Ordering::Relaxed) as u32;
                let total = self.export.total.max(1) as u32;
                ui.add(
                    egui::ProgressBar::new(done as f32 / total as f32)
                        .text(format!("Exporting: {done} / {total}")),
                );
            }
            if let Some(msg) = &self.export.message {
                ui.label(msg.clone());
            }
        });
        for a in pending {
            self.push(a);
        }
    }
}
