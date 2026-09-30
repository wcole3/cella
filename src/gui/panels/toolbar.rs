//! The top toolbar: play/pause, stepping, speed, zoom, reset, export and the
//! panel toggles — as icons with tooltips that name the keyboard shortcut.
//!
//! The toolbar changes nothing itself: every button pushes an
//! [`Action`](crate::gui::actions::Action) that the reducer applies after the
//! frame. The only state it edits directly is the "Run to +N" draft value,
//! which is a form field, not simulation state.

use crate::gui::actions::{Action, SnapshotChoice, speed_of};
use crate::gui::app::CellaApp;
use crate::gui::shortcuts::tooltip;
use crate::gui::state::Pacing;
use egui::Context;

/// A progress bar reading "done/total" for a running GIF export.
pub(in crate::gui) fn export_progress_bar(done: usize, total: usize) -> egui::ProgressBar {
    egui::ProgressBar::new(crate::gui::export::progress_fraction(done, total))
        .text(format!("{done}/{total}"))
}

/// The toolbar's icons. The proportional font family egui ships with (Ubuntu,
/// Noto Emoji, emoji-icon-font) lacks many box and arrow symbols, and a glyph
/// it cannot draw shows as an empty rectangle; icon buttons therefore render
/// in the monospace family, whose Hack font fills most of the gaps. A test
/// checks every icon here against those fonts.
const ICON_PAUSE: &str = "\u{23F8}";
const ICON_PLAY: &str = "\u{25B6}";
const ICON_STEP: &str = "\u{23ED}";
const ICON_RUN_TO: &str = "\u{23E9}";
const ICON_ZOOM_OUT: &str = "\u{2212}";
const ICON_FIT: &str = "\u{26F6}";
const ICON_RESET: &str = "\u{21BA}";
const ICON_SAVE: &str = "\u{1F4BE}";
const ICON_LEFT: &str = "\u{25E7}";
const ICON_RIGHT: &str = "\u{25E8}";
const ICON_HELP: &str = "?";

#[cfg(test)]
const ICONS: [&str; 11] = [
    ICON_PAUSE,
    ICON_PLAY,
    ICON_STEP,
    ICON_RUN_TO,
    ICON_ZOOM_OUT,
    ICON_FIT,
    ICON_RESET,
    ICON_SAVE,
    ICON_LEFT,
    ICON_RIGHT,
    ICON_HELP,
];

/// A toolbar button: an icon glyph, a tooltip with its shortcut, one action.
fn icon_button(ui: &mut egui::Ui, ctx: &Context, glyph: &str, label: &str, action: Action) -> bool {
    ui.button(egui::RichText::new(glyph).monospace())
        .on_hover_text(tooltip(ctx, label, &action))
        .clicked()
}

impl CellaApp {
    /// Build the top toolbar.
    ///
    /// Layout, left to right: the control-panel toggle sits flush left, above
    /// the panel it hides; then playback, speed, zoom, reset, export and help
    /// in separated groups; the workbench toggle sits flush right, above the
    /// workbench. Both toggles therefore line up with the panel edges.
    pub(in crate::gui) fn ui_top_controls(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let mut pending: Vec<Action> = Vec::new();
        ui.horizontal(|ui| {
            let left_label = if self.chrome.left_open {
                "Hide the control panel"
            } else {
                "Show the control panel"
            };
            if icon_button(ui, ctx, ICON_LEFT, left_label, Action::ToggleLeft) {
                pending.push(Action::ToggleLeft);
            }
            ui.separator();

            // The workbench toggle is laid out from the right edge so it
            // stays above the right panel whatever the window width.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let right_label = if self.chrome.right_open {
                    "Hide the workbench (rule, model, explore)"
                } else {
                    "Show the workbench (rule, model, explore)"
                };
                if icon_button(ui, ctx, ICON_RIGHT, right_label, Action::ToggleRight) {
                    pending.push(Action::ToggleRight);
                }
                ui.separator();
                if icon_button(
                    ui,
                    ctx,
                    ICON_HELP,
                    "Keyboard shortcuts",
                    Action::ToggleShortcuts,
                ) {
                    pending.push(Action::ToggleShortcuts);
                }
                ui.separator();

                // Everything else flows left to right in the remaining space.
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    self.ui_toolbar_groups(ui, ctx, &mut pending);
                });
            });
        });
        for a in pending {
            self.push(a);
        }
    }

    /// Playback, speed, zoom, reset and export controls, in separated groups.
    fn ui_toolbar_groups(&mut self, ui: &mut egui::Ui, ctx: &Context, pending: &mut Vec<Action>) {
        let (glyph, label) = if self.playback.playing {
            (ICON_PAUSE, "Pause")
        } else {
            (ICON_PLAY, "Play")
        };
        if icon_button(ui, ctx, glyph, label, Action::TogglePlay) {
            pending.push(Action::TogglePlay);
        }
        if icon_button(ui, ctx, ICON_STEP, "Step once", Action::Step) {
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
            ICON_RUN_TO,
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

        if icon_button(ui, ctx, ICON_ZOOM_OUT, "Zoom out", Action::ZoomOut) {
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
            ICON_FIT,
            "Zoom to fit the whole grid",
            Action::ZoomToFit,
        ) {
            pending.push(Action::ZoomToFit);
        }
        ui.separator();

        if icon_button(
            ui,
            ctx,
            ICON_RESET,
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
        if let Some((done, total)) = self.export_progress() {
            ui.add(export_progress_bar(done, total).desired_width(120.0));
        }
        if icon_button(
            ui,
            ctx,
            ICON_SAVE,
            "Save the current state as JSON",
            Action::SaveFinalState,
        ) {
            pending.push(Action::SaveFinalState);
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

    /// The "resume mid-run, or start over" prompt for a config loaded with a
    /// `snapshot` past step 0. Modelled on `ui_shortcuts_overlay` (a modal,
    /// closable via Escape or a click outside), except its buttons queue an
    /// [`Action::ResolveSnapshotLoad`] rather than mutate the scenario
    /// directly — resolving a snapshot load rebuilds the grid, which only
    /// the reducer in `apply_action` is allowed to do.
    pub(in crate::gui) fn ui_snapshot_load_modal(&mut self, ctx: &Context) {
        let Some(pending) = &self.chrome.pending_snapshot_load else {
            return;
        };
        let step = pending.cfg.snapshot().map(|s| s.step).unwrap_or(0);
        let name = pending.name.clone();
        let mut choice = None;
        let response = egui::Modal::new(egui::Id::new("snapshot_load_modal")).show(ctx, |ui| {
            ui.heading("Resume this run?");
            ui.label(format!("'{name}' was saved at step {step}."));
            ui.add_space(crate::gui::theme::SPACE_MD);
            ui.horizontal(|ui| {
                if ui.button(format!("Resume at step {step}")).clicked() {
                    choice = Some(SnapshotChoice::Resume);
                }
                if ui.button("Start from initial (step 0)").clicked() {
                    choice = Some(SnapshotChoice::Initial);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(SnapshotChoice::Cancel);
                }
            });
        });
        if response.should_close() {
            choice.get_or_insert(SnapshotChoice::Cancel);
        }
        if let Some(choice) = choice {
            self.push(Action::ResolveSnapshotLoad(choice));
        }
    }

    /// The GIF export options: frame count, frame rate and (for 1D) row
    /// stacking, asked for after the file is chosen. A modal like
    /// `ui_snapshot_load_modal`: Escape or a click outside cancels. The
    /// number fields are form drafts edited in place; the buttons queue
    /// [`Action::ConfirmExportGif`] / [`Action::CancelExportGif`].
    pub(in crate::gui) fn ui_export_modal(&mut self, ctx: &Context) {
        let is_1d = matches!(self.scenario.dim, Some(crate::gui::app::Dim::D1));
        let Some(pending) = &mut self.export.pending else {
            return;
        };
        let with_history = &mut self.export.with_history_1d;
        let mut choice = None;
        let response = egui::Modal::new(egui::Id::new("export_modal")).show(ctx, |ui| {
            ui.heading("Export GIF");
            ui.label(pending.path.display().to_string());
            ui.add_space(crate::gui::theme::SPACE_MD);
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut pending.steps)
                        .range(1..=crate::gui::export::MAX_EXPORT_STEPS),
                );
                ui.label("steps");
                ui.add(
                    egui::DragValue::new(&mut pending.fps)
                        .range(1..=crate::gui::export::MAX_EXPORT_FPS),
                );
                ui.label("fps");
            });
            ui.checkbox(&mut pending.looping, "Loop forever")
                .on_hover_text("Off: the GIF plays once and stops on its last frame.");
            if is_1d {
                ui.checkbox(with_history, "Stack rows into a space-time image")
                    .on_hover_text("Height is limited by the 1D history limit.");
            }
            ui.add_space(crate::gui::theme::SPACE_MD);
            ui.horizontal(|ui| {
                if ui.button("Export").clicked() {
                    choice = Some(Action::ConfirmExportGif);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(Action::CancelExportGif);
                }
            });
        });
        if response.should_close() {
            choice.get_or_insert(Action::CancelExportGif);
        }
        if let Some(a) = choice {
            self.push(a);
        }
    }

    /// The notice popup (see [`Notice`]). Waits while the snapshot-load
    /// question is open, so only one popup shows at a time.
    pub(in crate::gui) fn ui_notice_modal(&mut self, ctx: &Context) {
        if self.chrome.pending_snapshot_load.is_some() {
            return;
        }
        let Some(notice) = self.chrome.notice.clone() else {
            return;
        };
        let mut dismiss = false;
        let response = egui::Modal::new(egui::Id::new("notice_modal")).show(ctx, |ui| {
            ui.heading(&notice.title);
            for line in &notice.lines {
                ui.label(line);
            }
            ui.add_space(crate::gui::theme::SPACE_MD);
            if ui.button("OK").clicked() {
                dismiss = true;
            }
        });
        if dismiss || response.should_close() {
            self.push(Action::DismissNotice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::sim::tests::test_app;

    /// Every toolbar icon must exist in the monospace family egui ships with
    /// (Hack + Ubuntu + Noto Emoji + emoji-icon-font), or it draws as a box.
    /// `glyph_width` is used rather than `has_glyph`, which in egui 0.35
    /// reports `false` for any character the family's *first* font owns.
    #[test]
    fn every_toolbar_icon_is_in_the_default_fonts() {
        use egui::epaint::text::{FontDefinitions, Fonts, FontsView, TextOptions};
        let mut all = Fonts::new(TextOptions::default(), FontDefinitions::default());
        let mut fonts = all.with_pixels_per_point(1.0);
        let drawable = |fonts: &mut FontsView<'_>, family: egui::FontFamily, s: &str| {
            let id = egui::FontId::new(14.0, family);
            s.chars().all(|c| fonts.glyph_width(&id, c) > 0.0)
        };
        let mono = egui::FontFamily::Monospace;
        let prop = egui::FontFamily::Proportional;
        assert!(
            drawable(&mut fonts, mono.clone(), "A?"),
            "sanity: letters draw"
        );
        for icon in ICONS {
            assert!(
                drawable(&mut fonts, mono.clone(), icon),
                "toolbar icon {icon:?} has no glyph"
            );
        }
        // The old fit glyph is the reason this test exists.
        assert!(!drawable(&mut fonts, mono, "\u{2922}"));
        // The "remove axis" glyph in the Explore tab is drawn proportionally.
        assert!(drawable(&mut fonts, prop.clone(), "\u{2716}"));
        // The panel glyphs exist only in Hack, so they need the monospace family.
        assert!(!drawable(&mut fonts, prop, "\u{25E7}"));
    }

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

    #[test]
    fn the_export_modal_and_progress_bars_draw_headless() {
        let mut app = test_app();
        app.load_demo_life();
        app.export.pending = Some(crate::gui::state::PendingExport {
            path: "x.gif".into(),
            steps: 5,
            fps: 10,
            looping: true,
        });
        egui::__run_test_ctx(|ctx| app.ui_export_modal(ctx));
        assert!(app.actions.is_empty(), "nothing was clicked");
        app.load_demo_1d_rule30();
        egui::__run_test_ctx(|ctx| app.ui_export_modal(ctx));
        app.export.pending = None;
        egui::__run_test_ctx(|ctx| app.ui_export_modal(ctx));
        app.export.progress = Some(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(2)));
        app.export.total = 4;
        egui::__run_test_ui(|ui| {
            let ctx = ui.ctx().clone();
            app.ui_top_controls(ui, &ctx);
            app.ui_status_bar(ui);
        });
    }
}
