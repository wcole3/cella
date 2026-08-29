//! Loading and reshaping what the simulation is running.
//!
//! Everything here answers "what grid are we looking at": the built-in demos,
//! loading a JSON config from disk, resizing the grid, and resetting back to
//! the state a scenario started in.

use std::time::Duration;

use super::app::{CellaApp, Dim};
use crate::demos::{
    build_1d_code_n, build_1d_rule30, build_2d_life, build_2d_straightline,
    build_2d_three_state_cycle,
};
use cella_lib::*;
use rfd::FileDialog;

impl CellaApp {
    /// Resize the current grid to `grid_width` x `grid_height`, preserving existing
    /// cell data where it overlaps and filling new cells with Inactive.
    pub(in crate::gui) fn resize_grid(&mut self) {
        let new_w = self.inputs.grid_width.max(1);
        let new_h = self.inputs.grid_height.max(1);
        match self.scenario.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.scenario.d1 {
                    let old_w = g.width;
                    let rule = g.rule.clone();
                    let hist = g.history_limit;
                    let mut init: Vec<CellType> = vec![CellType::inactive(); new_w];
                    for (x, cell) in init.iter_mut().take(new_w.min(old_w)).enumerate() {
                        *cell = g.cell_type(x);
                    }
                    self.scenario.d1 = Some(Grid1D::new(new_w, hist, init, rule));
                    self.scenario.initial_state =
                        self.scenario.d1.as_ref().map(GridState::from_grid1d);
                    self.view.history_1d.clear();
                    self.edit.undo_stack.clear();
                    self.edit.current_paint_batch = None;
                    self.stats_clear_and_init();
                    self.set_status(format!("Resized 1D grid to width {}", new_w));
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.scenario.d2 {
                    let old_w = g.width;
                    let old_h = g.height;
                    let rule = g.rule.clone();
                    let hist = g.history_limit;
                    let mut init: Vec<CellType> = vec![CellType::inactive(); new_w * new_h];
                    for y in 0..new_h.min(old_h) {
                        for x in 0..new_w.min(old_w) {
                            init[y * new_w + x] = g.cell_type(y * old_w + x);
                        }
                    }
                    self.scenario.d2 = Some(Grid2D::new(new_w, new_h, hist, init, rule));
                    self.scenario.initial_state =
                        self.scenario.d2.as_ref().map(GridState::from_grid2d);
                    self.view.history_1d.clear();
                    self.edit.undo_stack.clear();
                    self.edit.current_paint_batch = None;
                    self.stats_clear_and_init();
                    self.set_status(format!("Resized 2D grid to {}×{}", new_w, new_h));
                }
            }
            None => {}
        }
    }
    // ----- Scenario loading -----
    pub(in crate::gui) fn load_demo_life(&mut self) {
        let (w, h, hist) = (50usize, 30usize, 5usize);
        self.scenario.d1 = None;
        self.scenario.dim = Some(Dim::D2);
        self.scenario.d2 = Some(build_2d_life(w, h, hist));
        self.scenario.initial_state = self.scenario.d2.as_ref().map(GridState::from_grid2d);
        self.view.history_1d.clear();
        self.edit.undo_stack.clear();
        self.edit.current_paint_batch = None;
        self.view.colors.clear();
        self.inputs.grid_width = w;
        self.inputs.grid_height = h;
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
        self.set_status("Loaded demo: Life (2D)");
    }
    pub(in crate::gui) fn load_demo_1d_rule30(&mut self) {
        let width = 201usize;
        let hist = 5usize;
        self.scenario.d2 = None;
        self.scenario.dim = Some(Dim::D1);
        self.scenario.d1 = Some(build_1d_rule30(width, hist));
        self.set_status("Loaded demo: 1D Rule 30");
        if let Some(g) = &self.scenario.d1 {
            self.scenario.initial_state = Some(GridState::from_grid1d(g));
        }
        self.view.history_1d.clear();
        self.edit.undo_stack.clear();
        self.edit.current_paint_batch = None;
        self.view.colors.clear();
        self.inputs.grid_width = width;
        self.inputs.grid_height = 1;
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }
    pub(in crate::gui) fn load_demo_1d_n2(&mut self) {
        let code: u128 = 0xAAAAAAAA;
        let width = 201usize;
        let hist = 5usize;
        self.scenario.d2 = None;
        self.scenario.dim = Some(Dim::D1);
        self.scenario.d1 =
            Some(build_1d_code_n(code, 2, width, hist).expect("n2 builder should validate"));
        if let Some(g) = &self.scenario.d1 {
            self.scenario.initial_state = Some(GridState::from_grid1d(g));
        }
        self.view.history_1d.clear();
        self.edit.undo_stack.clear();
        self.edit.current_paint_batch = None;
        self.view.colors.clear();
        self.inputs.grid_width = width;
        self.inputs.grid_height = 1;
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }
    pub(in crate::gui) fn load_demo_2d_three_state_cycle(&mut self) {
        let (w, h, hist) = (48usize, 27usize, 3usize);
        self.scenario.d1 = None;
        self.scenario.dim = Some(Dim::D2);
        self.scenario.d2 = Some(build_2d_three_state_cycle(w, h, hist));
        self.set_status("Loaded demo: 2D three-state cycle");
        self.scenario.initial_state = self.scenario.d2.as_ref().map(GridState::from_grid2d);
        self.view.history_1d.clear();
        self.edit.undo_stack.clear();
        self.edit.current_paint_batch = None;
        self.view.colors.clear();
        self.inputs.grid_width = w;
        self.inputs.grid_height = h;
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }
    pub(in crate::gui) fn load_demo_2d_straightline(&mut self) {
        let (w, h, hist) = (48usize, 27usize, 3usize);
        self.scenario.d1 = None;
        self.scenario.dim = Some(Dim::D2);
        self.scenario.d2 = Some(build_2d_straightline(w, h, hist));
        self.set_status("Loaded demo: 2D StraightLine");
        self.scenario.initial_state = self.scenario.d2.as_ref().map(GridState::from_grid2d);
        self.view.history_1d.clear();
        self.edit.undo_stack.clear();
        self.edit.current_paint_batch = None;
        self.view.colors.clear();
        self.inputs.grid_width = w;
        self.inputs.grid_height = h;
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }
    pub(in crate::gui) fn load_demo_1d_custom_from_inputs(&mut self) {
        let wolfram_code: u128 = self.inputs.custom_code.trim().parse().unwrap_or(30);
        let n: u8 = if self.inputs.custom_n == 0 {
            1
        } else {
            self.inputs.custom_n
        };
        let width = 201usize;
        let hist = 5usize;
        if let Ok(grid) = build_1d_code_n(wolfram_code, n, width, hist) {
            self.scenario.d2 = None;
            self.scenario.dim = Some(Dim::D1);
            self.scenario.d1 = Some(grid);
            self.set_status(format!("Loaded custom 1D: code={}, n={}", wolfram_code, n));
            if let Some(g) = &self.scenario.d1 {
                self.scenario.initial_state = Some(GridState::from_grid1d(g));
            }
            self.view.history_1d.clear();
            self.edit.undo_stack.clear();
            self.edit.current_paint_batch = None;
            self.view.colors.clear();
            self.inputs.grid_width = width;
            self.inputs.grid_height = 1;
            self.update_selected_draw_type_default();
            self.stats_clear_and_init();
            self.refresh_rule_editor_from_current();
        }
    }
    pub(in crate::gui) fn load_config_dialog(&mut self) {
        if let Some(path) = FileDialog::new().add_filter("json", &["json"]).pick_file() {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("config.json")
                .to_string();
            match config::CellaConfig::from_file(&path) {
                Ok(cfg) => match cfg {
                    config::CellaConfig::D1(_) => {
                        if let Some(g) = cfg.build_grid1d() {
                            self.scenario.dim = Some(Dim::D1);
                            self.scenario.d1 = Some(g);
                            self.scenario.d2 = None;
                            self.set_status(format!("Loaded config (1D): {}", name));
                            if let Some(gr) = &self.scenario.d1 {
                                self.scenario.initial_state = Some(GridState::from_grid1d(gr));
                                self.inputs.grid_width = gr.width;
                                self.inputs.grid_height = 1;
                            }
                            self.view.history_1d.clear();
                            self.edit.undo_stack.clear();
                            self.edit.current_paint_batch = None;
                            self.view.colors.clear();
                            self.update_selected_draw_type_default();
                            self.stats_clear_and_init();
                        }
                    }
                    config::CellaConfig::D2(_) => {
                        if let Some(g) = cfg.build_grid2d() {
                            self.scenario.dim = Some(Dim::D2);
                            self.scenario.d2 = Some(g);
                            self.scenario.d1 = None;
                            self.set_status(format!("Loaded config (2D): {}", name));
                            if let Some(gr) = &self.scenario.d2 {
                                self.scenario.initial_state = Some(GridState::from_grid2d(gr));
                                self.inputs.grid_width = gr.width;
                                self.inputs.grid_height = gr.height;
                            }
                            self.view.history_1d.clear();
                            self.edit.undo_stack.clear();
                            self.edit.current_paint_batch = None;
                            self.view.colors.clear();
                            self.update_selected_draw_type_default();
                            self.stats_clear_and_init();
                        }
                    }
                },
                Err(e) => {
                    let msg = format!("Failed to load config: {}", e);
                    eprintln!("{}", msg);
                    self.set_status(msg);
                }
            }
            // After loading any config, sync the rule editor
            self.refresh_rule_editor_from_current();
        }
    }
    /// Reset the current grid to its initial snapshot captured on load.
    pub(in crate::gui) fn reset_to_initial(&mut self) {
        // Cancel first: cancelling restores whatever play state a pending
        // "Run to +N" interrupted, and Reset always stops.
        self.cancel_run_to();
        self.playback.playing = false;
        // Reset simulation timer
        self.playback.elapsed = Duration::ZERO;
        self.playback.timed_steps = 0;
        self.playback.play_start = None;
        self.view.history_1d.clear();
        self.edit.undo_stack.clear();
        self.edit.current_paint_batch = None;
        if let Some(st) = &self.scenario.initial_state {
            match st {
                GridState::D1 { .. } => {
                    if let Some(g) = Grid1D::from_state(st) {
                        self.scenario.dim = Some(Dim::D1);
                        self.scenario.d1 = Some(g);
                        self.scenario.d2 = None;
                    }
                }
                GridState::D2 { .. } => {
                    if let Some(g) = Grid2D::from_state(st) {
                        self.scenario.dim = Some(Dim::D2);
                        self.scenario.d2 = Some(g);
                        self.scenario.d1 = None;
                    }
                }
            }
        }
        // Update default draw type after resetting
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
        self.set_status("Reset to initial state");
    }
}
