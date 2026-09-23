//! The Scenario tab: demo buttons, loading a config, the custom 1D builder,
//! the 1D history depth and grid resizing.

use crate::gui::actions::{Action, Demo};
use crate::gui::app::{CellaApp, Dim};
use crate::gui::interact::GridDims;
use crate::gui::theme::section;
use egui::TextEdit;

impl CellaApp {
    pub(in crate::gui) fn ui_dataset_controls(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Action> = Vec::new();
        section(ui, "Load", |ui| {
            if ui.button("Load config JSON\u{2026}").clicked() {
                pending.push(Action::LoadConfigDialog);
            }
            for (demo, label) in [
                (Demo::Life, "Demo: Life (2D)"),
                (Demo::Rule30, "Demo: 1D Rule 30"),
                (Demo::Radius2, "Demo: 1D radius 2"),
                (Demo::ThreeState2D, "Demo: 2D three-state"),
                (Demo::StraightLine2D, "Demo: 2D straight line"),
            ] {
                if ui.button(label).clicked() {
                    pending.push(Action::LoadDemo(demo));
                }
            }
        });
        section(ui, "Custom 1D rule", |ui| {
            ui.horizontal(|ui| {
                ui.label("code");
                ui.add_sized(
                    [90.0, 20.0],
                    TextEdit::singleline(&mut self.inputs.custom_code),
                );
                ui.label("n");
                ui.add(egui::DragValue::new(&mut self.inputs.custom_n).range(1..=3));
                if ui.button("Build").clicked() {
                    pending.push(Action::Build1D {
                        code: self.inputs.custom_code.clone(),
                        n: self.inputs.custom_n,
                    });
                }
            });
            ui.small("A Wolfram code over a window of 2n+1 cells (n = 1: 8 bits, up to 255).");
        });
        section(ui, "Grid", |ui| {
            if matches!(self.scenario.dim, Some(Dim::D1)) {
                ui.horizontal(|ui| {
                    ui.label("1D history rows");
                    let mut limit = self.view.history_limit_1d;
                    if ui
                        .add(egui::DragValue::new(&mut limit).range(1..=10_000))
                        .changed()
                    {
                        pending.push(Action::SetHistoryLimit1D(limit));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("W");
                ui.add(egui::DragValue::new(&mut self.inputs.grid_width).range(1..=2000));
                if matches!(self.scenario.dim, Some(Dim::D2)) {
                    ui.label("H");
                    ui.add(egui::DragValue::new(&mut self.inputs.grid_height).range(1..=2000));
                }
                let target = match self.scenario.dim {
                    Some(Dim::D2) => (self.inputs.grid_width, self.inputs.grid_height),
                    _ => (self.inputs.grid_width, 1),
                };
                // Height is always 1 for a 1D grid; `grid_dims`'s D1 case
                // reports history rows there instead, so translate it here
                // rather than adding a second `(width, height)` accessor.
                let current = match self.grid_dims() {
                    Some(GridDims::D1 { width, .. }) => Some((width, 1)),
                    Some(GridDims::D2 { width, height }) => Some((width, height)),
                    None => None,
                };
                let changed = current.is_some_and(|d| d != target);
                if ui
                    .add_enabled(changed, egui::Button::new("Resize"))
                    .on_hover_text(
                        "Change the grid size. Cells, ages and history in the overlap are kept \
                         (anchored top-left); new cells are Inactive. The step, rule, seed and \
                         model carry on.",
                    )
                    .on_disabled_hover_text("The grid is already this size.")
                    .clicked()
                {
                    pending.push(Action::Resize {
                        w: self.inputs.grid_width,
                        h: self.inputs.grid_height,
                    });
                }
            });
        });
        for a in pending {
            self.push(a);
        }
    }
}
