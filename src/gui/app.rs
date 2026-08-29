//! Core GUI application built on [`eframe`] / [`egui`].
//!
//! [`CellaApp`] is the main application struct that implements `eframe::App`.
//! It manages simulation state, rule editing, grid rendering, playback
//! controls, statistics, drawing/painting, and GIF export.

use super::state::{
    Chrome, EditState, EditorState, ExportState, Inputs, Playback, Scenario, StatsState,
    ViewSettings,
};
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::render::color_for;
use cella_lib::*;
use egui::scroll_area::{DragScroll, ScrollSource};
use egui::{Color32, Context};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::gui) enum Dim {
    D1,
    D2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::gui) enum DrawMode {
    Cycle,
    Paint,
}

/// Run the native GUI application.
pub fn run_gui(size: Option<(f32, f32)>) -> eframe::Result<()> {
    // Configure the initial window via NativeOptions/ViewportBuilder.
    // If size is provided, use it; otherwise default to a 16:9 reasonable size.
    let (mut w, mut h) = if let Some((w, h)) = size {
        (w, h)
    } else {
        let aspect = 16.0 / 9.0;
        let w = 1280.0_f32; // default width
        let h = w / aspect;
        (w, h)
    };
    // Clamp minimums for the initial inner size
    w = w.max(800.0);
    h = h.max(450.0);

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(egui::vec2(w, h))
        .with_min_inner_size(egui::vec2(800.0, 450.0))
        .with_title("Cella GUI")
        .with_resizable(true);

    let options = eframe::NativeOptions {
        viewport,
        ..eframe::NativeOptions::default()
    };

    eframe::run_native(
        "Cella GUI",
        options,
        Box::new(|cc| Ok(Box::new(CellaApp::new(cc)))),
    )
}

/// Main GUI application state and logic.
///
/// This struct owns the currently loaded grid (1D or 2D), UI state such as
/// play/pause, per-type colors, and render/export settings.
pub(in crate::gui) struct CellaApp {
    /// What is being simulated.
    pub(in crate::gui) scenario: Scenario,
    /// Play/pause, run-to, and the step stopwatch.
    pub(in crate::gui) playback: Playback,
    /// Zoom, colours, and the 1D history strip.
    pub(in crate::gui) view: ViewSettings,
    /// Painting tools and the undo stack.
    pub(in crate::gui) edit: EditState,
    /// GIF export settings and its worker thread.
    pub(in crate::gui) export: ExportState,
    /// Population series behind the chart.
    pub(in crate::gui) stats: StatsState,
    /// The rule editor's working copy.
    pub(in crate::gui) editor: EditorState,
    /// Text scaling and the status line.
    pub(in crate::gui) chrome: Chrome,
    /// Pending values typed into form fields.
    pub(in crate::gui) inputs: Inputs,
}

impl CellaApp {
    pub(in crate::gui) fn set_status<S: Into<String>>(&mut self, msg: S) {
        self.chrome.status_message = Some(msg.into());
    }

    pub(in crate::gui) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            scenario: Scenario::default(),
            playback: Playback::default(),
            view: ViewSettings::default(),
            edit: EditState::default(),
            export: ExportState::default(),
            stats: StatsState::default(),
            editor: EditorState::default(),
            chrome: Chrome::new(&cc.egui_ctx),
            inputs: Inputs::default(),
        };
        // Start with a default 2D Life-like demo
        app.load_demo_life();
        app
    }

    #[inline]
    pub(in crate::gui) fn inactive_color(&self) -> Color32 {
        self.view.inactive_color
    }

    pub(in crate::gui) fn set_color_for(&mut self, ty: &CellType, color: Color32) {
        if ty.as_str() == INACTIVE {
            self.view.inactive_color = color;
            return;
        }
        self.view.colors.insert(ty.0, color);
    }

    pub(in crate::gui) fn color_of(&self, ty: &CellType) -> Color32 {
        color_for(
            *ty,
            &self.view.colors,
            &self.view.palette,
            self.inactive_color(),
        )
    }

    /// Apply user font scaling to egui text styles.
    ///
    /// Restyling forces egui to re-layout every galley, so this is a no-op unless
    /// the scale actually changed since the last frame.
    pub(in crate::gui) fn apply_font_scale(&mut self, ctx: &Context) {
        if self.chrome.font_scale == self.chrome.applied_font_scale {
            return;
        }
        self.chrome.applied_font_scale = self.chrome.font_scale;
        let mut map = self.chrome.base_text_styles.clone();
        for font in map.values_mut() {
            font.size = (font.size * self.chrome.font_scale).max(6.0);
        }
        ctx.all_styles_mut(|style| style.text_styles = map.clone());
    }
}

impl CellaApp {
    /// The right-hand rule editor panel, which can be collapsed by the
    /// toolbar toggle or by dragging its edge past the minimum width.
    pub(in crate::gui) fn ui_rule_editor_panel(&mut self, ui: &mut egui::Ui) {
        // Right-side Rule Editor panel (resizable, can be hidden via toggle).
        // `show_collapsible` animates the slide in/out and lets a drag past the
        // minimum width collapse the panel, keeping `editor.visible` in sync with
        // the toolbar toggle.
        // Held in a local because `show_collapsible` writes back through the `&mut bool`
        // (drag-to-close), which would otherwise alias the `&mut self` the body needs.
        let mut show_editor = self.editor.visible;
        egui::Panel::right("right_rule_editor")
            .resizable(true)
            .min_size(220.0)
            .default_size(340.0)
            .show_collapsible(ui, &mut show_editor, |ui| {
                ui.heading("Rule Editor");
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.ui_rule_editor(ui);
                    });
            });
        self.editor.visible = show_editor;
    }

    /// The left-hand control column: scenario loading, UI settings, editing
    /// tools, export, colours, and statistics.
    pub(in crate::gui) fn ui_left_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("left_controls")
            .default_size(260.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.ui_dataset_controls(ui);
                        ui.separator();
                        ui.collapsing("UI Settings", |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("A-").clicked() {
                                    self.chrome.font_scale =
                                        (self.chrome.font_scale - 0.1).max(0.5);
                                }
                                if ui.button("A+").clicked() {
                                    self.chrome.font_scale =
                                        (self.chrome.font_scale + 0.1).min(3.0);
                                }
                                ui.label(format!("Font: {:.0}%", self.chrome.font_scale * 100.0));
                            });
                            ui.add(
                                egui::Slider::new(&mut self.chrome.font_scale, 0.5..=3.0)
                                    .text("Font scale"),
                            );
                        });
                        ui.separator();
                        ui.collapsing("Editing", |ui| {
                            ui.horizontal(|ui| {
                                let is_cycle = matches!(self.edit.draw_mode, DrawMode::Cycle);
                                if ui.radio(is_cycle, "Cycle").clicked() {
                                    self.edit.draw_mode = DrawMode::Cycle;
                                }
                                let is_paint = matches!(self.edit.draw_mode, DrawMode::Paint);
                                if ui.radio(is_paint, "Paint").clicked() {
                                    self.edit.draw_mode = DrawMode::Paint;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Paint type:");
                                // Build a type list from rule/config declared types (not just currently present)
                                let mut names: Vec<String> = self
                                    .declared_types()
                                    .into_iter()
                                    .map(|t| t.as_str().to_string())
                                    .collect();
                                // Ensure ordering with Inactive first
                                names.sort();
                                names.sort_by_key(|a| a != INACTIVE);
                                let current_name = self
                                    .edit
                                    .selected_draw_type
                                    .as_ref()
                                    .map(|t| t.as_str())
                                    .unwrap_or_else(|| INACTIVE);
                                let mut sel = current_name;
                                egui::ComboBox::from_label("").selected_text(sel).show_ui(
                                    ui,
                                    |ui| {
                                        for n in &names {
                                            ui.selectable_value(&mut sel, n.as_str(), n);
                                        }
                                    },
                                );
                                if sel != current_name {
                                    self.edit.selected_draw_type = Some(CellType::from(sel));
                                }
                            });
                            ui.label("Hold and drag on the grid while paused to paint.");
                        });
                        ui.separator();
                        ui.collapsing("Export", |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.export.steps).range(1..=10_000),
                                );
                                ui.label("steps");
                            });
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut self.export.fps).range(1..=60));
                                ui.label("fps");
                            });
                            ui.collapsing("Options", |ui| {
                                ui.horizontal(|ui| {
                                    let mut flag = self.export.with_history_1d;
                                    if ui
                                        .checkbox(&mut flag, "1D GIF: include vertical history")
                                        .changed()
                                    {
                                        self.export.with_history_1d = flag;
                                    }
                                });
                                ui.small(
                                    "Applies to 1D GIF export; height limited by 1D history limit.",
                                );
                            });
                            ui.separator();
                            if let Some(p) = &self.export.progress {
                                let done = p.load(Ordering::Relaxed) as u32;
                                let total = self.export.total.max(1) as u32;
                                let frac = (done as f32) / (total as f32);
                                ui.add(
                                    egui::ProgressBar::new(frac)
                                        .text(format!("Exporting: {} / {}", done, total)),
                                );
                            }
                            if let Some(msg) = &self.export.message {
                                ui.label(msg.clone());
                            }
                        });
                        ui.separator();
                        self.ui_colors(ui);
                        ui.separator();
                        self.ui_statistics(ui);
                    });
            });
    }

    /// The bottom status strip: step counter, run timer, and the latest message.
    pub(in crate::gui) fn ui_status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("bottom_status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Step: {}", self.current_step()));
                // Show simulation timer when steps have been timed
                if self.playback.timed_steps > 0 || self.playback.play_start.is_some() {
                    let total = if let Some(start) = self.playback.play_start {
                        self.playback.elapsed + start.elapsed()
                    } else {
                        self.playback.elapsed
                    };
                    let total_secs = total.as_secs_f64();
                    let steps = self.playback.timed_steps.max(1);
                    let avg_ms = (total_secs * 1000.0) / steps as f64;
                    ui.separator();
                    ui.label(format!("Time: {:.2}s", total_secs));
                    ui.label(format!("Avg: {:.2} ms/step", avg_ms));
                }
                if let Some(msg) = &self.chrome.status_message {
                    ui.separator();
                    ui.label(egui::RichText::new(msg.clone()).italics());
                }
            });
        });
    }

    /// The central grid viewport, plus every gesture that acts on it.
    /// The central grid viewport, plus every gesture that acts on it.
    ///
    /// The gesture handlers live in [`super::interact`]; this only decides the
    /// order they run in and how the scroll area is configured.
    pub(in crate::gui) fn ui_viewport(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            self.handle_hotkeys(ui);
            egui::ScrollArea::both()
                // Left-drag is reserved for painting, so never drag-to-scroll.
                .scroll_source(ScrollSource {
                    drag: DragScroll::Never,
                    scroll_bar: true,
                    mouse_wheel: true,
                })
                .show(ui, |ui| match self.paint_grid_viewport(ui) {
                    Some(response) => {
                        self.handle_zoom(ui, &response);
                        self.handle_pan(ui, &response);
                        self.handle_paint(ui, &response);
                        self.handle_cycle_click(&response);
                    }
                    None => {
                        ui.label("No grid loaded.");
                    }
                });
        });
    }

    /// Collect a finished GIF export thread and report how it went.
    pub(in crate::gui) fn poll_export(&mut self) {
        // If an export thread is active, poll for completion and finalize
        if let Some(handle) = &self.export.join
            && handle.is_finished()
        {
            if let Some(handle) = self.export.join.take() {
                match handle
                    .join()
                    .unwrap_or_else(|_| Err("export thread panicked".to_string()))
                {
                    Ok(()) => {
                        self.export.message = Some("Export complete".into());
                        self.set_status("Export complete");
                    }
                    Err(e) => {
                        let msg = format!("Export failed: {}", e);
                        self.export.message = Some(msg.clone());
                        self.set_status(msg);
                    }
                }
            }
            self.export.progress = None;
            self.export.total = 0;
        }
    }

    /// Ask egui to wake up again only when something is actually animating.
    ///
    /// `stepped` is what [`CellaApp::tick_play`] just reported. The panels were
    /// built before it ran, so a frame that stepped is already out of date the
    /// moment it is drawn — including the frame a "Run to +N" finishes on,
    /// which clears the run and so matches none of the branches below. Without
    /// this the screen would keep showing the old step counter and "Running
    /// to N" until the next mouse move.
    pub(in crate::gui) fn request_next_repaint(&self, ctx: &Context, stepped: bool) {
        // Only drive continuous repaints when something is actually animating.
        // Previously this pinned the app at ~100 fps (and full CPU/GPU) even while
        // paused with nothing on screen changing; egui repaints on input anyway.
        if stepped {
            // Draw the state this frame's steps produced.
            ctx.request_repaint();
        } else if self.burst_target().is_some() {
            // Bursting: come straight back so consecutive frame budgets run
            // back-to-back and throughput is set by the engine, not the clock.
            ctx.request_repaint();
        } else if self.playback.playing {
            // Wake up in time for the next simulation tick, capped so the timer
            // readout in the status bar still updates smoothly.
            ctx.request_repaint_after(Duration::from_millis(self.playback.refresh_ms.min(100)));
        } else if self.export.join.is_some() {
            // Poll the export thread's progress a few times a second.
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl eframe::App for CellaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        self.apply_font_scale(ctx);

        egui::Panel::top("top_controls").show(ui, |ui| {
            self.ui_top_controls(ui, ctx);
        });
        self.ui_left_panel(ui);
        self.ui_rule_editor_panel(ui);
        self.ui_status_bar(ui);
        self.ui_viewport(ui);

        self.poll_export();
        let stepped = self.tick_play();
        self.request_next_repaint(ctx, stepped);
    }
}
