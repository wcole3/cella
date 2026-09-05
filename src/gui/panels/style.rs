//! The Style tab: theme, text size, colours and grid lines.

use crate::gui::actions::Action;
use crate::gui::app::CellaApp;
use crate::gui::theme::{ThemeChoice, section};
use cella_lib::CellType;

impl CellaApp {
    pub(in crate::gui) fn ui_style_tab(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Action> = Vec::new();
        section(ui, "Theme", |ui| {
            ui.horizontal(|ui| {
                for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                    if ui
                        .selectable_label(self.chrome.theme == choice, choice.label())
                        .clicked()
                    {
                        pending.push(Action::SetTheme(choice));
                    }
                }
            });
            ui.horizontal(|ui| {
                let f = self.chrome.font_scale;
                if ui.button("A-").clicked() {
                    pending.push(Action::SetFontScale(f - 0.1));
                }
                if ui.button("A+").clicked() {
                    pending.push(Action::SetFontScale(f + 0.1));
                }
                ui.label(format!("Text {:.0}%", f * 100.0));
            });
            let mut f = self.chrome.font_scale;
            if ui
                .add(egui::Slider::new(&mut f, 0.5..=3.0).text("Text size"))
                .changed()
            {
                pending.push(Action::SetFontScale(f));
            }
        });
        section(ui, "Grid", |ui| {
            let mut lines = self.view.show_grid_lines;
            if ui.checkbox(&mut lines, "Grid lines").changed() {
                pending.push(Action::ToggleGridLines);
            }
            let mut col = self.view.grid_line_color;
            ui.horizontal(|ui| {
                ui.label("Grid line colour");
                if ui.color_edit_button_srgba(&mut col).changed() {
                    pending.push(Action::SetGridLineColor(col));
                }
            });
        });
        section(ui, "Colours", |ui| {
            let mut inactive = self.view.inactive_color;
            ui.horizontal(|ui| {
                if ui.color_edit_button_srgba(&mut inactive).changed() {
                    pending.push(Action::SetInactiveColor(inactive));
                }
                ui.label("Inactive (background)");
            });
            // Every declared type, so a type that has died out keeps its swatch.
            for ty in self.declared_types() {
                if ty == CellType::inactive() {
                    continue;
                }
                let mut col = self.color_of(&ty);
                ui.horizontal(|ui| {
                    if ui.color_edit_button_srgba(&mut col).changed() {
                        pending.push(Action::SetTypeColor(ty, col));
                    }
                    ui.label(ty.as_str());
                });
            }
        });
        for a in pending {
            self.push(a);
        }
    }
}
