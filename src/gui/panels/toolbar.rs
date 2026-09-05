//! The top toolbar: play/pause, stepping, speed, zoom, reset, export and the
//! panel toggles — as icons with tooltips that name the keyboard shortcut.
//!
//! The toolbar changes nothing itself: every button pushes an
//! [`Action`](crate::gui::actions::Action) that the reducer applies after the
//! frame. The only state it edits directly is the "Run to +N" draft value,
//! which is a form field, not simulation state.

use crate::gui::actions::{Action, speed_of};
use crate::gui::app::CellaApp;
use crate::gui::shortcuts::tooltip;
use crate::gui::state::Pacing;
use egui::Context;

/// A toolbar button: an icon glyph, a tooltip with its shortcut, one action.
fn icon_button(ui: &mut egui::Ui, ctx: &Context, glyph: &str, label: &str, action: Action) -> bool {
    ui.button(glyph)
        .on_hover_text(tooltip(ctx, label, &action))
        .clicked()
}

impl CellaApp {
    /// Build the top toolbar.
    pub(in crate::gui) fn ui_top_controls(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let mut pending: Vec<Action> = Vec::new();
        ui.horizontal(|ui| {
            let (glyph, label) = if self.playback.playing {
                ("\u{23F8}", "Pause")
            } else {
                ("\u{25B6}", "Play")
            };
            if icon_button(ui, ctx, glyph, label, Action::TogglePlay) {
                pending.push(Action::TogglePlay);
            }
            if icon_button(ui, ctx, "\u{23ED}", "Step once", Action::Step) {
                pending.push(Action::Step);
            }
            ui.add(
                egui::DragValue::new(&mut self.playback.run_to_steps)
                    .range(1..=1_000_000)
                    .prefix("+"),
            )
            .on_hover_text("How many steps \u{23E9} runs, as fast as possible");
            if icon_button(
                ui,
                ctx,
                "\u{23E9}",
                "Run ahead by the steps shown",
                Action::RunTo { steps: 0 },
            ) {
                pending.push(Action::RunTo {
                    steps: self.playback.run_to_steps,
                });
            }
            ui.separator();

            // Speed: a log slider in steps per second, plus Max. The stored
            // field is still `refresh_ms`; the slider is a view of it.
            let is_max = self.playback.pacing == Pacing::Unbounded;
            let mut sps = speed_of(self.playback.refresh_ms);
            let slider = egui::Slider::new(&mut sps, 1.0..=1000.0)
                .logarithmic(true)
                .suffix(" steps/s")
                .fixed_decimals(0)
                .text("Speed");
            let resp = ui.add_enabled(!is_max, slider).on_hover_text(
                "Paced playback: steps per second. Frame-bound above about 60; \
                 tick Max to run as fast as the machine allows.",
            );
            if resp.changed() {
                pending.push(Action::SetSpeed { steps_per_s: sps });
            }
            let mut max = is_max;
            if ui
                .toggle_value(&mut max, "Max")
                .on_hover_text("Run as many steps per frame as fit in the time budget")
                .changed()
            {
                pending.push(Action::SetMaxSpeed(max));
            }
            ui.separator();

            if icon_button(ui, ctx, "\u{2212}", "Zoom out", Action::ZoomOut) {
                pending.push(Action::ZoomOut);
            }
            let mut scale = self.view.scale;
            if ui
                .add(egui::DragValue::new(&mut scale).range(1..=64).suffix(" px"))
                .on_hover_text("Pixels per cell")
                .changed()
            {
                pending.push(Action::SetScale(scale));
            }
            if icon_button(ui, ctx, "+", "Zoom in", Action::ZoomIn) {
                pending.push(Action::ZoomIn);
            }
            if icon_button(
                ui,
                ctx,
                "\u{2922}",
                "Zoom to fit the whole grid",
                Action::ZoomToFit,
            ) {
                pending.push(Action::ZoomToFit);
            }
            ui.separator();

            if icon_button(
                ui,
                ctx,
                "\u{21BA}",
                "Reset to the initial state",
                Action::Reset,
            ) {
                pending.push(Action::Reset);
            }
            let exporting = self.export.join.is_some();
            let export_btn = ui
                .add_enabled(!exporting, egui::Button::new("GIF"))
                .on_hover_text(tooltip(ctx, "Export an animated GIF", &Action::ExportGif));
            if export_btn.clicked() {
                pending.push(Action::ExportGif);
            }
            if exporting {
                ui.label("Exporting\u{2026}");
            }
            if icon_button(
                ui,
                ctx,
                "\u{1F4BE}",
                "Save the current state as JSON",
                Action::SaveFinalState,
            ) {
                pending.push(Action::SaveFinalState);
            }
            ui.separator();

            if icon_button(
                ui,
                ctx,
                "\u{25E7}",
                "Show or hide the left panel",
                Action::ToggleLeft,
            ) {
                pending.push(Action::ToggleLeft);
            }
            if icon_button(
                ui,
                ctx,
                "\u{25E8}",
                "Show or hide the rule editor",
                Action::ToggleRight,
            ) {
                pending.push(Action::ToggleRight);
            }
            if icon_button(ui, ctx, "?", "Keyboard shortcuts", Action::ToggleShortcuts) {
                pending.push(Action::ToggleShortcuts);
            }
        });
        for a in pending {
            self.push(a);
        }
    }

    /// The `?` overlay: every shortcut, from the same table the keys use.
    /// Closes on Escape, a click outside, or the button.
    pub(in crate::gui) fn ui_shortcuts_overlay(&mut self, ctx: &Context) {
        if !self.chrome.show_shortcuts {
            return;
        }
        let response = egui::Modal::new(egui::Id::new("shortcuts_overlay")).show(ctx, |ui| {
            ui.heading("Keyboard shortcuts");
            ui.add_space(crate::gui::theme::SPACE_MD);
            egui::Grid::new("shortcuts_grid")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for s in crate::gui::shortcuts::shortcuts() {
                        ui.monospace(ctx.format_shortcut(&s.keys));
                        ui.label(if s.while_playing {
                            s.label.to_string()
                        } else {
                            format!("{} (while paused)", s.label)
                        });
                        ui.end_row();
                    }
                });
            ui.add_space(crate::gui::theme::SPACE_MD);
            ui.small("Keys are ignored while a text field has the keyboard.");
            if ui.button("Close").clicked() {
                self.chrome.show_shortcuts = false;
            }
        });
        if response.should_close() {
            self.chrome.show_shortcuts = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::gui::sim::tests::test_app;

    #[test]
    fn the_toolbar_draws_headless_and_only_queues_actions() {
        let mut app = test_app();
        app.load_demo_life();
        let scale = app.view.scale;
        let ctx = egui::Context::default();
        egui::__run_test_ui(|ui| {
            let ctx = ui.ctx().clone();
            app.ui_top_controls(ui, &ctx);
        });
        assert_eq!(app.view.scale, scale, "drawing changes nothing");
        assert!(app.actions.is_empty(), "nothing was clicked");
        app.chrome.show_shortcuts = true;
        egui::__run_test_ctx(|ctx| app.ui_shortcuts_overlay(ctx));
        let _ = ctx;
    }
}
