//! Actions: the one way the panels change the simulation.
//!
//! Panels and the toolbar do not reach into playback, the grid or the view
//! and poke fields. They describe what the user wants as an [`Action`] and
//! push it onto a queue; after the frame is drawn, [`CellaApp::drain_actions`]
//! hands each one to [`CellaApp::apply_action`], the single place where state
//! changes. If you know React, this is the reducer: widgets emit, one function
//! applies, and the state is only ever written in one place.
//!
//! Why bother? Three things get easier:
//! - **Testing.** Every behaviour behind a button or key is a plain function
//!   call on a plain struct — no window, no egui.
//! - **Reuse.** A keyboard shortcut and a toolbar button do the same thing by
//!   pushing the same action, so they cannot drift apart.
//! - **Borrowing.** A panel that holds a borrow (the rule editor's working
//!   copy, say) cannot also mutate the grid mid-draw; queuing sidesteps that.
//!
//! Panel bodies may still edit their own *drafts* directly (text being typed,
//! a slider's local copy). Anything that touches `scenario`, `playback`,
//! `view`, the undo stack or a worker goes through an action.

use super::app::{CellaApp, Dim, DrawMode};
use super::interact::{GridDims, MAX_SCALE, MIN_SCALE};
use super::state::{ControlTab, Pacing, WorkbenchTab};
use super::theme::ThemeChoice;
use cella_lib::types::interner;
use cella_lib::{CellType, INACTIVE, ParamValue};
use egui::Color32;

/// The built-in demo scenarios.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::gui) enum Demo {
    Life,
    Rule30,
    Radius2,
    ThreeState2D,
    StraightLine2D,
}

/// Something the user asked for. Cheap to clone; carries its own data.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) enum Action {
    // ── playback ──
    TogglePlay,
    Step,
    /// Burst `steps` steps ahead of the current step.
    RunTo {
        steps: u64,
    },
    /// Paced playback speed in steps per second (1..=1000).
    SetSpeed {
        steps_per_s: f64,
    },
    /// `true`: run as fast as the machine allows; `false`: back to paced.
    SetMaxSpeed(bool),
    Reset,
    // ── view ──
    ZoomIn,
    ZoomOut,
    /// Pick the largest cell size at which the whole grid fits the viewport.
    ZoomToFit,
    SetScale(usize),
    ToggleGridLines,
    SetFontScale(f32),
    /// Switch the theme. Background and grid-line colours still at the old
    /// theme's defaults follow it; colours the user picked stay.
    SetTheme(ThemeChoice),
    SetGridLineColor(Color32),
    SetInactiveColor(Color32),
    SetTypeColor(CellType, Color32),
    SetHistoryLimit1D(usize),
    // ── edit ──
    Undo,
    SetDrawMode(DrawMode),
    SetDrawType(CellType),
    // ── scenario / rule / model ──
    LoadDemo(Demo),
    LoadConfigDialog,
    /// Rebuild the grid at a new size, keeping the cells that overlap.
    Resize {
        w: usize,
        h: usize,
    },
    /// Build the custom 1D scenario from a Wolfram code and radius.
    Build1D {
        code: String,
        n: u8,
    },
    /// Write the rule editor's working copy into the grid.
    ApplyRule,
    ApplyModelParam {
        key: String,
        value: ParamValue,
    },
    /// Declare a new cell type for the rule editor.
    AddType(String),
    // ── chrome / io ──
    SetControlTab(ControlTab),
    SetWorkbenchTab(WorkbenchTab),
    ToggleLeft,
    ToggleRight,
    ToggleShortcuts,
    ExportGif,
    SaveFinalState,
}

/// Most actions applied in one frame before the drain gives up; a reducer arm
/// that keeps enqueuing itself would otherwise hang the UI.
const DRAIN_LIMIT: usize = 1024;

/// Paced playback speed for a refresh interval.
pub(in crate::gui) fn speed_of(refresh_ms: u64) -> f64 {
    1000.0 / refresh_ms.max(1) as f64
}

/// Refresh interval for a paced playback speed, kept within 1..=1000 ms
/// (1 to 1000 steps per second).
pub(in crate::gui) fn refresh_ms_of(steps_per_s: f64) -> u64 {
    if !steps_per_s.is_finite() || steps_per_s <= 0.0 {
        return 1000;
    }
    (1000.0 / steps_per_s).round().clamp(1.0, 1000.0) as u64
}

/// The largest whole cell size (pixels) at which a `grid_w × grid_h` grid
/// fits inside `available`, leaving a small margin for scrollbars.
pub(in crate::gui) fn fit_scale(available: egui::Vec2, grid_w: usize, grid_h: usize) -> usize {
    const MARGIN: f32 = 16.0;
    if !available.x.is_finite() || !available.y.is_finite() {
        return MIN_SCALE;
    }
    let w = ((available.x - MARGIN) / grid_w.max(1) as f32).floor();
    let h = ((available.y - MARGIN) / grid_h.max(1) as f32).floor();
    let s = w.min(h);
    if !s.is_finite() || s < MIN_SCALE as f32 {
        return MIN_SCALE;
    }
    (s as usize).clamp(MIN_SCALE, MAX_SCALE)
}

impl CellaApp {
    /// Queue an action for the end of this frame.
    pub(in crate::gui) fn push(&mut self, action: Action) {
        self.actions.push_back(action);
    }

    /// Apply every queued action, in order. Arms may enqueue further actions.
    pub(in crate::gui) fn drain_actions(&mut self) {
        let mut applied = 0;
        while let Some(a) = self.actions.pop_front() {
            self.apply_action(a);
            applied += 1;
            if applied >= DRAIN_LIMIT {
                self.actions.clear();
                self.set_status("Too many actions in one frame; the rest were dropped");
                break;
            }
        }
    }

    /// The reducer: the only place an action becomes a state change.
    pub(in crate::gui) fn apply_action(&mut self, action: Action) {
        match action {
            Action::TogglePlay => self.toggle_play(),
            Action::Step => {
                self.step_once();
                self.set_status(format!("Stepped to {}", self.current_step()));
            }
            Action::RunTo { steps } => {
                self.playback.run_to_steps = steps.max(1);
                self.start_run_to();
            }
            Action::SetSpeed { steps_per_s } => {
                self.playback.refresh_ms = refresh_ms_of(steps_per_s);
            }
            Action::SetMaxSpeed(on) => {
                self.playback.pacing = if on {
                    Pacing::Unbounded
                } else {
                    Pacing::Interval
                };
            }
            Action::Reset => self.reset_to_initial(),
            Action::ZoomIn => self.view.scale = (self.view.scale + 1).min(MAX_SCALE),
            Action::ZoomOut => self.view.scale = self.view.scale.saturating_sub(1).max(MIN_SCALE),
            Action::ZoomToFit => {
                if let (Some(avail), Some(dims)) = (self.view.last_viewport_size, self.grid_dims())
                {
                    let (w, h) = match dims {
                        GridDims::D1 { width, .. } => (width, self.view.history_limit_1d + 1),
                        GridDims::D2 { width, height } => (width, height),
                    };
                    self.view.scale = fit_scale(avail, w, h);
                    self.set_status(format!("Zoom to fit: {} px per cell", self.view.scale));
                }
            }
            Action::SetScale(s) => self.view.scale = s.clamp(MIN_SCALE, MAX_SCALE),
            Action::ToggleGridLines => self.view.show_grid_lines = !self.view.show_grid_lines,
            Action::SetFontScale(f) => self.chrome.font_scale = f.clamp(0.5, 3.0),
            Action::SetTheme(theme) => {
                let old = self.chrome.theme;
                if self.view.inactive_color == old.default_inactive() {
                    self.view.inactive_color = theme.default_inactive();
                }
                if self.view.grid_line_color == old.default_grid_line() {
                    self.view.grid_line_color = theme.default_grid_line();
                }
                self.chrome.theme = theme;
            }
            Action::SetGridLineColor(c) => self.view.grid_line_color = c,
            Action::SetInactiveColor(c) => self.view.inactive_color = c,
            Action::SetTypeColor(ty, c) => self.set_color_for(&ty, c),
            Action::SetHistoryLimit1D(n) => self.view.history_limit_1d = n.clamp(1, 10_000),
            Action::Undo => self.undo_last_batch(),
            Action::SetDrawMode(mode) => self.edit.draw_mode = mode,
            Action::SetDrawType(ty) => self.edit.selected_draw_type = Some(ty),
            Action::LoadDemo(demo) => match demo {
                Demo::Life => self.load_demo_life(),
                Demo::Rule30 => self.load_demo_1d_rule30(),
                Demo::Radius2 => self.load_demo_1d_n2(),
                Demo::ThreeState2D => self.load_demo_2d_three_state_cycle(),
                Demo::StraightLine2D => self.load_demo_2d_straightline(),
            },
            Action::LoadConfigDialog => self.load_config_dialog(),
            Action::Resize { w, h } => {
                self.inputs.grid_width = w.max(1);
                self.inputs.grid_height = h.max(1);
                self.resize_grid();
            }
            Action::Build1D { code, n } => {
                self.inputs.custom_code = code;
                self.inputs.custom_n = n.max(1);
                self.load_demo_1d_custom_from_inputs();
            }
            Action::ApplyRule => self.apply_rule_draft(),
            Action::ApplyModelParam { key, value } => self.apply_model_param(&key, value),
            Action::AddType(name) => {
                let name = name.trim();
                if !name.is_empty() && name != INACTIVE {
                    self.editor
                        .custom_types
                        .insert(interner().get_or_intern(name));
                    self.set_status(format!("Added type '{name}'"));
                }
            }
            Action::SetControlTab(tab) => self.chrome.control_tab = tab,
            Action::SetWorkbenchTab(tab) => self.chrome.workbench_tab = tab,
            Action::ToggleLeft => self.chrome.left_open = !self.chrome.left_open,
            Action::ToggleRight => self.chrome.right_open = !self.chrome.right_open,
            Action::ToggleShortcuts => self.chrome.show_shortcuts = !self.chrome.show_shortcuts,
            Action::ExportGif => self.export_gif_dialog(),
            Action::SaveFinalState => self.save_final_state(),
        }
    }

    /// Write the rule editor's working copy into the live grid. A draft the
    /// library refuses stays in the editor with the reason shown; a good one
    /// becomes the grid's rule and the editor re-reads it.
    fn apply_rule_draft(&mut self) {
        let result = match self.scenario.dim {
            Some(Dim::D1) => self
                .editor
                .rule_1d
                .as_ref()
                .map(|e| e.to_rule().map(|r| (Some(r), None))),
            Some(Dim::D2) => self
                .editor
                .rule_2d
                .as_ref()
                .map(|e| e.to_rule().map(|r| (None, Some(r)))),
            None => None,
        };
        match result {
            Some(Ok((r1, r2))) => {
                if let (Some(r), Some(g)) = (r1, self.scenario.d1.as_mut()) {
                    g.rule = r;
                }
                if let (Some(r), Some(g)) = (r2, self.scenario.d2.as_mut()) {
                    g.rule = r;
                }
                self.editor.error_msg = None;
                self.refresh_rule_editor_from_current();
                self.set_status("Applied rule");
            }
            Some(Err(e)) => {
                self.editor.error_msg = Some(e.clone());
                self.set_status(format!("Rule error: {e}"));
            }
            None => {}
        }
    }

    /// Undo the most recent edit batch, while paused.
    fn undo_last_batch(&mut self) {
        if self.playback.playing {
            return;
        }
        let Some(batch) = self.edit.undo_stack.pop() else {
            return;
        };
        let limit = match self.grid_dims() {
            Some(GridDims::D1 { width, .. }) => width,
            Some(GridDims::D2 { width, height }) => width * height,
            None => return,
        };
        for (idx, prev) in batch {
            // Skip out-of-range entries, but stop entirely at the first entry
            // the engine rejects.
            if idx < limit && !self.set_cell(idx, prev, "Undo") {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::sim::tests::test_app;
    use cella_lib::CellType;

    #[test]
    fn speed_and_refresh_interval_convert_both_ways_within_bounds() {
        assert_eq!(refresh_ms_of(10.0), 100);
        assert_eq!(speed_of(100), 10.0);
        assert_eq!(refresh_ms_of(1.0), 1000);
        assert_eq!(refresh_ms_of(1000.0), 1);
        assert_eq!(refresh_ms_of(5000.0), 1, "capped at 1000 steps/s");
        assert_eq!(refresh_ms_of(0.1), 1000, "floored at 1 step/s");
        assert_eq!(refresh_ms_of(f64::NAN), 1000);
        assert_eq!(refresh_ms_of(-3.0), 1000);
        assert_eq!(
            speed_of(0),
            1000.0,
            "a zero interval reads as the fastest pace"
        );
        for ms in [1u64, 7, 33, 250, 1000] {
            assert_eq!(refresh_ms_of(speed_of(ms)), ms);
        }
    }

    #[test]
    fn fit_scale_takes_the_tighter_axis_and_stays_in_bounds() {
        assert_eq!(fit_scale(egui::vec2(816.0, 616.0), 100, 100), 6);
        assert_eq!(
            fit_scale(egui::vec2(816.0, 216.0), 100, 100),
            2,
            "height is the tighter axis"
        );
        assert_eq!(
            fit_scale(egui::vec2(10.0, 10.0), 100, 100),
            MIN_SCALE,
            "never below 1"
        );
        assert_eq!(
            fit_scale(egui::vec2(10_000.0, 10_000.0), 4, 4),
            MAX_SCALE,
            "never above 64"
        );
        assert_eq!(fit_scale(egui::vec2(f32::NAN, 100.0), 4, 4), MIN_SCALE);
        assert_eq!(
            fit_scale(egui::vec2(116.0, 116.0), 0, 0),
            64.min(MAX_SCALE),
            "empty grids do not divide by zero"
        );
    }

    #[test]
    fn playback_actions_drive_the_clock_fields() {
        let mut app = test_app();
        app.load_demo_life();
        app.apply_action(Action::SetSpeed { steps_per_s: 10.0 });
        assert_eq!(app.playback.refresh_ms, 100);
        app.apply_action(Action::SetMaxSpeed(true));
        assert_eq!(app.playback.pacing, Pacing::Unbounded);
        assert!(app.burst_target().is_none(), "not playing yet");
        app.apply_action(Action::TogglePlay);
        assert!(app.playback.playing);
        assert_eq!(
            app.burst_target(),
            Some(u64::MAX),
            "Max + Play bursts without a target"
        );
        app.apply_action(Action::SetMaxSpeed(false));
        assert_eq!(app.playback.pacing, Pacing::Interval);
        app.apply_action(Action::TogglePlay);
        assert!(!app.playback.playing);
        let before = app.current_step();
        app.apply_action(Action::Step);
        assert_eq!(app.current_step(), before + 1);
        app.apply_action(Action::RunTo { steps: 7 });
        assert_eq!(app.playback.run_to_target, Some(before + 8));
        assert!(app.playback.playing);
        app.apply_action(Action::Reset);
        assert!(!app.playback.playing);
        assert_eq!(app.current_step(), 0);
        assert!(app.playback.run_to_target.is_none());
    }

    #[test]
    fn view_actions_clamp_and_fit() {
        let mut app = test_app();
        app.load_demo_life();
        app.view.scale = MAX_SCALE;
        app.apply_action(Action::ZoomIn);
        assert_eq!(app.view.scale, MAX_SCALE);
        app.view.scale = MIN_SCALE;
        app.apply_action(Action::ZoomOut);
        assert_eq!(app.view.scale, MIN_SCALE);
        app.apply_action(Action::SetScale(999));
        assert_eq!(app.view.scale, MAX_SCALE);
        app.apply_action(Action::SetScale(0));
        assert_eq!(app.view.scale, MIN_SCALE);
        // Zoom to fit needs last frame's viewport size; without it, nothing happens.
        app.view.scale = 3;
        app.apply_action(Action::ZoomToFit);
        assert_eq!(app.view.scale, 3);
        app.view.last_viewport_size = Some(egui::vec2(516.0, 316.0));
        app.apply_action(Action::ZoomToFit);
        assert_eq!(
            app.view.scale, 10,
            "the 50x30 Life demo fits at 10 px per cell"
        );
        let lines = app.view.show_grid_lines;
        app.apply_action(Action::ToggleGridLines);
        assert_eq!(app.view.show_grid_lines, !lines);
        app.apply_action(Action::SetFontScale(9.0));
        assert_eq!(app.chrome.font_scale, 3.0);
        app.apply_action(Action::ToggleLeft);
        assert!(!app.chrome.left_open);
        app.apply_action(Action::ToggleRight);
        assert!(!app.chrome.right_open);
        app.apply_action(Action::ToggleShortcuts);
        assert!(app.chrome.show_shortcuts);
        app.apply_action(Action::SetControlTab(ControlTab::Style));
        assert_eq!(app.chrome.control_tab, ControlTab::Style);
        app.apply_action(Action::SetWorkbenchTab(WorkbenchTab::Explore));
        assert_eq!(app.chrome.workbench_tab, WorkbenchTab::Explore);
        let red = Color32::from_rgb(200, 0, 0);
        app.apply_action(Action::SetGridLineColor(red));
        assert_eq!(app.view.grid_line_color, red);
        app.apply_action(Action::SetInactiveColor(red));
        assert_eq!(app.view.inactive_color, red);
        let alive = CellType::from("Alive");
        app.apply_action(Action::SetTypeColor(alive, red));
        assert_eq!(app.color_of(&alive), red);
        app.apply_action(Action::SetHistoryLimit1D(0));
        assert_eq!(app.view.history_limit_1d, 1);
    }

    #[test]
    fn scenario_edit_and_rule_actions_change_the_grid() {
        let mut app = test_app();
        app.apply_action(Action::LoadDemo(Demo::Rule30));
        assert_eq!(app.scenario.dim, Some(Dim::D1));
        app.apply_action(Action::LoadDemo(Demo::Radius2));
        assert_eq!(app.scenario.dim, Some(Dim::D1));
        app.apply_action(Action::LoadDemo(Demo::ThreeState2D));
        assert_eq!(app.scenario.dim, Some(Dim::D2));
        app.apply_action(Action::LoadDemo(Demo::StraightLine2D));
        assert_eq!(app.scenario.dim, Some(Dim::D2));
        app.apply_action(Action::LoadDemo(Demo::Life));
        assert_eq!(app.scenario.dim, Some(Dim::D2));
        app.apply_action(Action::Resize { w: 12, h: 7 });
        let g = app.scenario.d2.as_ref().unwrap();
        assert_eq!((g.width, g.height), (12, 7));
        app.apply_action(Action::Build1D {
            code: "110".into(),
            n: 1,
        });
        assert_eq!(app.scenario.dim, Some(Dim::D1));
        assert_eq!(
            app.scenario.d1.as_ref().unwrap().rule.subrules[0].wolfram_code,
            110
        );
        app.apply_action(Action::SetDrawMode(DrawMode::Paint));
        assert_eq!(app.edit.draw_mode, DrawMode::Paint);
        let x = CellType::from("X");
        app.apply_action(Action::SetDrawType(x));
        assert_eq!(app.edit.selected_draw_type, Some(x));
        app.apply_action(Action::AddType("  Ember ".into()));
        assert!(app.editor.custom_types.contains(&CellType::from("Ember").0));
        app.apply_action(Action::AddType(INACTIVE.into()));
        assert!(!app.editor.custom_types.contains(&CellType::inactive().0));

        // A rule draft goes live through ApplyRule; a bad one stays in the editor.
        app.apply_action(Action::LoadDemo(Demo::Life));
        {
            let draft = app.editor.rule_2d.as_mut().expect("draft mirrors the grid");
            draft.subrules[0].count = 5;
        }
        app.apply_action(Action::ApplyRule);
        assert_eq!(app.scenario.d2.as_ref().unwrap().rule.subrules[0].count, 5);
        assert!(app.editor.error_msg.is_none());
        {
            let draft = app.editor.rule_2d.as_mut().unwrap();
            draft.subrules[0].range = 0;
        }
        app.apply_action(Action::ApplyRule);
        assert!(app.editor.error_msg.is_some(), "range 0 is refused");
        assert_eq!(
            app.scenario.d2.as_ref().unwrap().rule.subrules[0].range,
            1,
            "grid untouched"
        );
        // Model parameters route through the same door.
        app.apply_action(Action::ApplyModelParam {
            key: "p0".into(),
            value: ParamValue::Float(0.1),
        });
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("Model parameter error")
                || app.scenario.d2.as_ref().unwrap().model.is_none()
        );
    }

    #[test]
    fn switching_theme_moves_default_colours_but_not_chosen_ones() {
        let mut app = test_app();
        assert_eq!(
            app.view.inactive_color,
            ThemeChoice::Dark.default_inactive()
        );
        app.apply_action(Action::SetTheme(ThemeChoice::Light));
        assert_eq!(app.chrome.theme, ThemeChoice::Light);
        assert_eq!(
            app.view.inactive_color,
            ThemeChoice::Light.default_inactive()
        );
        assert_eq!(
            app.view.grid_line_color,
            ThemeChoice::Light.default_grid_line()
        );
        // A colour the user picked is theirs.
        let mine = egui::Color32::from_rgb(1, 2, 3);
        app.view.inactive_color = mine;
        app.apply_action(Action::SetTheme(ThemeChoice::Dark));
        assert_eq!(app.view.inactive_color, mine);
        assert_eq!(
            app.view.grid_line_color,
            ThemeChoice::Dark.default_grid_line()
        );
    }

    #[test]
    fn undo_restores_the_last_edit_batch_only_while_paused() {
        let mut app = test_app();
        app.load_demo_life();
        let alive = CellType::from("Alive");
        let inactive = CellType::inactive();
        let prev = app.cell_type_at(0).unwrap();
        assert_eq!(prev, inactive);
        assert!(app.set_cell(0, alive, "test"));
        app.edit.undo_stack.push(vec![(0, prev)]);
        app.playback.playing = true;
        app.apply_action(Action::Undo);
        assert_eq!(app.cell_type_at(0), Some(alive), "no undo while playing");
        app.playback.playing = false;
        app.apply_action(Action::Undo);
        assert_eq!(app.cell_type_at(0), Some(inactive));
        app.apply_action(Action::Undo);
        assert_eq!(
            app.cell_type_at(0),
            Some(inactive),
            "an empty stack is a no-op"
        );
    }

    #[test]
    fn the_queue_drains_in_order_and_stops_at_the_limit() {
        let mut app = test_app();
        app.load_demo_life();
        app.push(Action::SetScale(5));
        app.push(Action::ZoomIn);
        app.push(Action::ToggleGridLines);
        let lines = app.view.show_grid_lines;
        app.drain_actions();
        assert_eq!(app.view.scale, 6);
        assert_eq!(app.view.show_grid_lines, !lines);
        assert!(app.actions.is_empty());
        for _ in 0..(DRAIN_LIMIT + 5) {
            app.push(Action::ZoomIn);
        }
        app.drain_actions();
        assert!(app.actions.is_empty());
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("Too many")
        );
    }
}
