//! Core GUI application built on [`eframe`] / [`egui`].
//!
//! [`CellaApp`] is the main application struct that implements `eframe::App`.
//! It manages simulation state, rule editing, grid rendering, playback
//! controls, statistics, drawing/painting, and GIF export.

use super::actions::Action;
use super::state::{
    Chrome, EditState, EditorState, ExportState, Inputs, Playback, Scenario, StatsState,
    ViewSettings,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::render::color_for;
use cella_lib::*;
use egui::scroll_area::{DragScroll, ScrollSource};
use egui::{Color32, Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::gui) enum Dim {
    D1,
    D2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::gui) enum DrawMode {
    Cycle,
    Paint,
    /// Click to drop the selected pattern (2D only).
    Stamp,
}

/// Run the native GUI application. `config` is an optional config file to
/// show instead of the Life demo (the `--config` command-line option).
pub fn run_gui(size: Option<(f32, f32)>, config: Option<PathBuf>) -> eframe::Result<()> {
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
        Box::new(move |cc| Ok(Box::new(CellaApp::new(cc, config.as_deref())))),
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
    /// The Explore tab: ensemble / evolution settings and its worker thread.
    pub(in crate::gui) explore: crate::gui::explore::ExploreState,
    /// Text scaling and the status line.
    pub(in crate::gui) chrome: Chrome,
    /// Pending values typed into form fields.
    pub(in crate::gui) inputs: Inputs,
    /// Actions queued by the panels this frame, applied by `drain_actions`.
    pub(in crate::gui) actions: VecDeque<Action>,
}

impl CellaApp {
    pub(in crate::gui) fn set_status<S: Into<String>>(&mut self, msg: S) {
        self.chrome.status_message = Some(msg.into());
    }

    pub(in crate::gui) fn new(cc: &eframe::CreationContext<'_>, config: Option<&Path>) -> Self {
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
            actions: VecDeque::new(),
            explore: crate::gui::explore::ExploreState::default(),
        };
        app.apply_startup_config(config);
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
    /// Push the design tokens for the chosen theme into egui, only on frames
    /// where the choice changed (the restyle is not free).
    pub(in crate::gui) fn apply_theme_if_changed(&mut self, ctx: &Context) {
        if self.chrome.applied_theme == Some(self.chrome.theme) {
            return;
        }
        super::theme::apply(ctx, self.chrome.theme);
        self.chrome.applied_theme = Some(self.chrome.theme);
    }

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
    /// The bottom status strip: step counter, run timer, and the latest message.
    pub(in crate::gui) fn ui_status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("bottom_status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Step: {}", self.current_step()));
                if self.playback.playing && self.playback.steps_per_s > 0.0 {
                    ui.separator();
                    ui.label(format!("{:.0} steps/s", self.playback.steps_per_s));
                }
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
            // Remembered for "zoom to fit", which the toolbar asks for before
            // this frame's viewport exists.
            self.view.last_viewport_size = Some(ui.available_size());
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
                        self.handle_stamp_click(&response);
                        self.draw_tool_ghost(ui, &response);
                        self.show_hover_inspector(ui, &response);
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
        } else if self.explore.worker.as_ref().is_some_and(|w| w.busy) {
            // The Explore worker wakes us itself after each message; this is
            // the safety net in case it is between messages for a while.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

impl eframe::App for CellaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        self.apply_font_scale(ctx);
        self.apply_theme_if_changed(ctx);

        egui::Panel::top("top_controls").show(ui, |ui| {
            self.ui_top_controls(ui, ctx);
        });
        self.ui_control_panel(ui);
        self.ui_workbench_panel(ui);
        self.ui_status_bar(ui);
        self.ui_viewport(ui);
        self.ui_shortcuts_overlay(ctx);
        self.ui_snapshot_load_modal(ctx);

        // Everything the panels asked for lands here, after they were drawn.
        self.drain_actions();
        self.poll_export();
        self.poll_explore();
        let stepped = self.tick_play();
        self.request_next_repaint(ctx, stepped);
    }
}
