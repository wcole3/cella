//! The Edit tab: which tool the mouse is, what it paints, and GIF export.

use crate::gui::actions::Action;
use crate::gui::app::{CellaApp, Dim, DrawMode};
use crate::gui::layers::Layer;
use crate::gui::patterns::PATTERNS;
use crate::gui::theme::section;
use cella_lib::{CellType, INACTIVE};
use std::sync::atomic::Ordering;

impl CellaApp {
    pub(in crate::gui) fn ui_edit_tab(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Action> = Vec::new();
        let is_2d = matches!(self.scenario.dim, Some(Dim::D2));
        section(ui, "Tool", |ui| {
            ui.horizontal(|ui| {
                let mut tools = vec![
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
                ];
                if is_2d {
                    tools.push((
                        DrawMode::Stamp,
                        "Stamp",
                        "Click to drop a pattern (its top-left corner lands on the cell)",
                    ));
                }
                for (mode, label, help) in tools {
                    if ui
                        .selectable_label(self.edit.draw_mode == mode, label)
                        .on_hover_text(help)
                        .clicked()
                    {
                        pending.push(Action::SetDrawMode(mode));
                    }
                }
            });
            if self.edit.draw_mode == DrawMode::Paint {
                let mut brush = self.edit.brush;
                if ui
                    .add(egui::Slider::new(&mut brush, 1..=15).text("Brush (cells)"))
                    .changed()
                {
                    pending.push(Action::SetBrush(brush));
                }
            }
            if self.edit.draw_mode == DrawMode::Stamp && is_2d {
                let current = PATTERNS.get(self.edit.stamp).map_or("", |p| p.name);
                let mut sel = self.edit.stamp;
                egui::ComboBox::from_id_salt("stamp_pattern")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (i, p) in PATTERNS.iter().enumerate() {
                            ui.selectable_value(&mut sel, i, p.name);
                        }
                    });
                if sel != self.edit.stamp {
                    pending.push(Action::SelectStamp(sel));
                }
            }
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
        section(ui, "Random fill", |ui| {
            ui.add(
                egui::Slider::new(&mut self.edit.fill_density, 0.0..=1.0).text("Share of cells"),
            );
            ui.horizontal(|ui| {
                ui.label("Type");
                let types: Vec<CellType> = self
                    .declared_types()
                    .into_iter()
                    .filter(|t| *t != CellType::inactive())
                    .collect();
                if self.edit.fill_type.is_none_or(|t| !types.contains(&t)) {
                    self.edit.fill_type = types.first().copied();
                }
                let current = self.edit.fill_type.map_or(INACTIVE, |t| t.as_str());
                egui::ComboBox::from_id_salt("fill_type")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for t in &types {
                            ui.selectable_value(&mut self.edit.fill_type, Some(*t), t.as_str());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Seed");
                ui.add(egui::DragValue::new(&mut self.edit.fill_seed));
                if ui.button("\u{21BB}").on_hover_text("Next seed").clicked() {
                    self.edit.fill_seed = self.edit.fill_seed.wrapping_add(1);
                }
                ui.checkbox(&mut self.edit.fill_clear, "Clear first")
                    .on_hover_text(
                        "Empty the grid before filling and make the result the new starting state",
                    );
            });
            let can_fill = self.edit.fill_type.is_some() && !self.playback.playing;
            if ui
                .add_enabled(can_fill, egui::Button::new("Fill"))
                .on_hover_text("Reproducible: the same seed always paints the same picture")
                .clicked()
                && let Some(ty) = self.edit.fill_type
            {
                pending.push(Action::RandomFill {
                    density: self.edit.fill_density,
                    ty,
                    seed: self.edit.fill_seed,
                    clear_first: self.edit.fill_clear,
                });
            }
        });
        section(ui, "Layers", |ui| {
            let mut lines = self.view.show_grid_lines;
            if ui.checkbox(&mut lines, "Grid lines").changed() {
                pending.push(Action::ToggleLayer(Layer::Grid));
            }
            let mut age = self.view.layers.age;
            if ui
                .checkbox(&mut age, "Age heat")
                .on_hover_text("Tint cells by how recently they changed: fronts glow")
                .changed()
            {
                pending.push(Action::ToggleLayer(Layer::Age));
            }
            if self.view.layers.age {
                let mut cap = self.view.layers.age_cap;
                ui.horizontal(|ui| {
                    ui.label("Fade after");
                    if ui
                        .add(egui::DragValue::new(&mut cap).range(1..=100_000))
                        .changed()
                    {
                        pending.push(Action::SetAgeCap(cap));
                    }
                    ui.label("steps");
                });
            }
            let has_map = self.view.layers.probability_map.is_some();
            let mut prob = self.view.layers.probability;
            if ui
                .add_enabled(has_map, egui::Checkbox::new(&mut prob, "Probability map"))
                .on_hover_text(
                    "The Explore ensemble's per-cell probability (needs a running ensemble)",
                )
                .changed()
            {
                pending.push(Action::ToggleLayer(Layer::Probability));
            }
            let mut opacity = self.view.layers.opacity;
            if ui
                .add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("Opacity"))
                .changed()
            {
                pending.push(Action::SetLayerOpacity(opacity));
            }
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
