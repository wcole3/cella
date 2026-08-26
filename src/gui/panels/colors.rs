//! Per-cell-type colour pickers, including the configurable Inactive colour.

use crate::gui::app::CellaApp;
use cella_lib::CellType;

impl CellaApp {
    pub(in crate::gui) fn ui_colors(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Colors", |ui| {
            // Inactive color editor (always visible)
            let mut inact = self.view.inactive_color;
            ui.horizontal(|ui| {
                ui.label("Inactive:");
                if ui.color_edit_button_srgba(&mut inact).changed() {
                    self.view.inactive_color = inact;
                }
            });
            ui.separator();
            // Colors for all declared types (from rules/config), not just those currently present
            let tys = self.declared_types();
            for ty in tys {
                if ty == CellType::inactive() {
                    continue;
                }
                let mut col = self.color_of(&ty);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    self.set_color_for(&ty, col);
                }
                ui.label(ty.as_str());
            }
        });
    }
    // ----- Save/Export -----
}
