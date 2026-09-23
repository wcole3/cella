//! Loading and reshaping what the simulation is running.
//!
//! Everything here answers "what grid are we looking at": the built-in demos,
//! loading a JSON config from disk, resizing the grid, and resetting back to
//! the state a scenario started in.

use std::path::Path;
use std::time::Duration;

use super::actions::SnapshotChoice;
use super::app::{CellaApp, Dim};
use super::render::{distinct_palette_slots, parse_hex_color};
use super::state::PendingSnapshotLoad;
use crate::demos::{
    build_1d_code_n, build_1d_rule30, build_2d_life, build_2d_straightline,
    build_2d_three_state_cycle,
};
use cella_lib::*;
use rfd::FileDialog;

impl CellaApp {
    /// Change the grid to `grid_width` x `grid_height` through the library's
    /// `resize`: the overlap keeps its cells, ages and history (anchored
    /// top-left), and the step, rule, seed and model carry on. The Reset
    /// target is resized the same way, so Reset returns to the resized start
    /// and a save's `initial` matches the new size. All or nothing: if the
    /// live grid or the Reset target refuses, nothing changes and the status
    /// bar says why.
    pub(in crate::gui) fn resize_grid(&mut self) {
        let (w, h) = (self.inputs.grid_width.max(1), self.inputs.grid_height.max(1));
        let result = match self.scenario.dim {
            Some(Dim::D1) => self.resized_1d(w),
            Some(Dim::D2) => self.resized_2d(w, h),
            None => return,
        };
        match result {
            Ok(size) => {
                // Paint undo stores flat cell indices, which a width change
                // would point at the wrong cells.
                self.edit.undo_stack.clear();
                self.edit.current_paint_batch = None;
                self.explore_on_grid_replaced();
                self.set_status(format!("Resized grid to {size}"));
            }
            Err(why) => self.set_status(format!("Resize failed: {why}")),
        }
    }

    /// Resize the live 1D grid and its Reset target together; nothing is
    /// stored unless both succeed.
    fn resized_1d(&mut self, w: usize) -> Result<String, String> {
        let Some(g) = &self.scenario.d1 else {
            return Err("no 1D grid is loaded".into());
        };
        let mut live = g.clone();
        live.resize(w).map_err(|e| e.to_string())?;
        let initial = match &self.scenario.initial_state {
            Some(st) => {
                let mut ig = Grid1D::from_state(st).ok_or("the Reset target could not be rebuilt")?;
                ig.resize(w).map_err(|e| e.to_string())?;
                Some(GridState::from_grid1d(&ig))
            }
            None => None,
        };
        self.scenario.d1 = Some(live);
        self.scenario.initial_state = initial;
        Ok(format!("width {w}"))
    }

    /// Resize the live 2D grid and its Reset target together; nothing is
    /// stored unless both succeed.
    fn resized_2d(&mut self, w: usize, h: usize) -> Result<String, String> {
        let Some(g) = &self.scenario.d2 else {
            return Err("no 2D grid is loaded".into());
        };
        let mut live = g.clone();
        live.resize(w, h).map_err(|e| e.to_string())?;
        let initial = match &self.scenario.initial_state {
            Some(st) => {
                let mut ig = Grid2D::from_state(st).ok_or("the Reset target could not be rebuilt")?;
                ig.resize(w, h).map_err(|e| e.to_string())?;
                Some(GridState::from_grid2d(&ig))
            }
            None => None,
        };
        self.scenario.d2 = Some(live);
        self.scenario.initial_state = initial;
        Ok(format!("{w}×{h}"))
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
        self.edit.rule_undo.clear();
        self.edit.current_paint_batch = None;
        self.reset_colors_for_scenario();
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
        self.edit.rule_undo.clear();
        self.edit.current_paint_batch = None;
        self.reset_colors_for_scenario();
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
        self.edit.rule_undo.clear();
        self.edit.current_paint_batch = None;
        self.reset_colors_for_scenario();
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
        self.edit.rule_undo.clear();
        self.edit.current_paint_batch = None;
        self.reset_colors_for_scenario();
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
        self.edit.rule_undo.clear();
        self.edit.current_paint_batch = None;
        self.reset_colors_for_scenario();
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
            self.edit.rule_undo.clear();
            self.edit.current_paint_batch = None;
            self.reset_colors_for_scenario();
            self.inputs.grid_width = width;
            self.inputs.grid_height = 1;
            self.update_selected_draw_type_default();
            self.stats_clear_and_init();
            self.refresh_rule_editor_from_current();
        }
    }
    /// "Load Config JSON..." button: ask for a file, then load it.
    pub(in crate::gui) fn load_config_dialog(&mut self) {
        match FileDialog::new().set_directory(std::env::current_dir().unwrap_or_default()).add_filter("json", &["json"]).pick_file() {
            Some(path) => self.load_config_from_path(&path),
            None => self.report_no_file_chosen("config"),
        }
    }

    /// Explain an empty file-dialog result.
    ///
    /// `rfd` answers `None` both when the user presses Cancel and when no dialog
    /// could be shown at all — on Linux it needs `xdg-desktop-portal` reachable
    /// over the D-Bus session bus, or the `zenity` program as a fallback; a bare
    /// WSL has neither. Cancel needs no message, but "nothing happened" does, so
    /// say what to do. `rfd` logs the underlying error at ERROR level (see
    /// `main` for where the logger is installed).
    pub(in crate::gui) fn report_no_file_chosen(&mut self, what: &str) {
        self.set_status(format!(
            "No {what} chosen. If no dialog appeared, this desktop has no file-dialog \
             service: start with --config <path>, or see docs/app.md (Troubleshooting)."
        ));
    }

    /// What the window shows at startup: the Life demo, with the given config
    /// file loaded on top of it when there is one. Loading the demo first means
    /// a bad `--config` path still leaves a working grid on screen, with the
    /// error in the status bar.
    pub(in crate::gui) fn apply_startup_config(&mut self, path: Option<&Path>) {
        self.load_demo_life();
        if let Some(p) = path {
            self.load_config_from_path(p);
        }
    }

    /// Load a config file, replacing the grid, the Reset snapshot, and the
    /// editor state. Reports success or failure in the status bar.
    ///
    /// A `snapshot` at step 0, or no `snapshot` at all, loads immediately.
    /// A `snapshot` past step 0 instead opens the "Resume at step N? / Start
    /// from initial?" modal (`ui_snapshot_load_modal` in `panels::toolbar`)
    /// and waits for [`Action::ResolveSnapshotLoad`] — the grid is untouched
    /// until the user answers.
    pub(in crate::gui) fn load_config_from_path(&mut self, path: &Path) {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("config.json")
            .to_string();
        match config::CellaConfig::from_file(path) {
            Ok(cfg) => match cfg.snapshot().map(|s| s.step) {
                Some(step) if step > 0 => {
                    self.chrome.pending_snapshot_load = Some(PendingSnapshotLoad { cfg, name });
                }
                _ => self.finish_config_load(cfg, &name, false),
            },
            Err(e) => {
                let msg = format!("Failed to load config: {}", e);
                eprintln!("{}", msg);
                self.set_status(msg);
            }
        }
        // After loading any config (or deferring to the modal), sync the
        // rule editor: harmless if nothing changed yet.
        self.refresh_rule_editor_from_current();
    }

    /// The modal's answer for a config with a pending mid-run `snapshot`.
    pub(in crate::gui) fn resolve_snapshot_load(&mut self, choice: SnapshotChoice) {
        let Some(pending) = self.chrome.pending_snapshot_load.take() else {
            return;
        };
        match choice {
            SnapshotChoice::Cancel => {}
            SnapshotChoice::Resume => self.finish_config_load(pending.cfg, &pending.name, true),
            SnapshotChoice::Initial => self.finish_config_load(pending.cfg, &pending.name, false),
        }
    }

    /// Report a config whose grid could not be built at all — every
    /// `build_grid1d`/`build_grid2d`/`_resumed` guard returned `None` (a
    /// wrong `initial` length, `history_limit > 255`, a `width * height`
    /// overflow, or a model that fails to attach). These guards turn what
    /// used to be a panic into a silent no-op, so without this the user
    /// would see nothing happen at all.
    fn report_invalid_config(&mut self, name: &str) {
        let msg = format!(
            "Failed to load config: {name} does not describe a valid grid \
             (check `initial` length, history_limit <= 255, width/height, model)"
        );
        eprintln!("{}", msg);
        self.set_status(msg);
    }

    /// Build the grid a loaded config describes — resumed mid-run when
    /// `resume` is set and the snapshot is usable, its `initial` cells
    /// otherwise — and swap it into the scenario. The Reset target is always
    /// `initial`, regardless of `resume`, so Reset returns to step 0 either
    /// way.
    fn finish_config_load(&mut self, cfg: config::CellaConfig, name: &str, resume: bool) {
        match cfg {
            config::CellaConfig::D1(_) => {
                let resumed = resume.then(|| cfg.build_grid1d_resumed()).flatten();
                let resumed_ok = resumed.is_some();
                let Some(g) = resumed.or_else(|| cfg.build_grid1d()) else {
                    self.report_invalid_config(name);
                    return;
                };
                let initial_state = cfg.build_grid1d().map(|ig| GridState::from_grid1d(&ig));
                let (w, h) = (g.width, 1);
                self.scenario.dim = Some(Dim::D1);
                self.scenario.d1 = Some(g);
                self.scenario.d2 = None;
                self.scenario.initial_state = initial_state;
                self.set_status(load_status_message("1D", name, resume, resumed_ok));
                self.finish_scenario_load(w, h, cfg.colors());
            }
            config::CellaConfig::D2(_) => {
                let resumed = resume.then(|| cfg.build_grid2d_resumed()).flatten();
                let resumed_ok = resumed.is_some();
                let Some(g) = resumed.or_else(|| cfg.build_grid2d()) else {
                    self.report_invalid_config(name);
                    return;
                };
                let initial_state = cfg.build_grid2d().map(|ig| GridState::from_grid2d(&ig));
                let (w, h) = (g.width, g.height);
                self.scenario.dim = Some(Dim::D2);
                self.scenario.d2 = Some(g);
                self.scenario.d1 = None;
                self.scenario.initial_state = initial_state;
                self.set_status(load_status_message("2D", name, resume, resumed_ok));
                self.finish_scenario_load(w, h, cfg.colors());
            }
        }
    }

    /// Common tail of a successful scenario swap, shared by every "Load
    /// scenario" path (demos, config loads, both snapshot-load choices):
    /// resize inputs, clear transient edit/view state, put the config's
    /// colours back on top of the automatic ones, and resync the draw-type
    /// default, stats and rule editor. `h` is 1 for a 1D grid.
    fn finish_scenario_load(
        &mut self,
        w: usize,
        h: usize,
        colors: &std::collections::BTreeMap<String, String>,
    ) {
        self.inputs.grid_width = w;
        self.inputs.grid_height = h;
        self.view.history_1d.clear();
        self.edit.undo_stack.clear();
        self.edit.rule_undo.clear();
        self.edit.current_paint_batch = None;
        self.reset_colors_for_scenario();
        self.apply_config_colors(colors);
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }

    /// Give every declared type its own colour for the scenario that was just
    /// loaded. Hashing names into eight palette slots collides easily (the
    /// wildfire demo had Forest and Burning both land on sky blue), so instead
    /// each type takes the next free slot, in the stable order `declared_types`
    /// returns. Explicit colours from a config are applied on top afterwards.
    pub(in crate::gui) fn reset_colors_for_scenario(&mut self) {
        self.view.colors.clear();
        self.view.config_colors.clear();
        let types: Vec<CellType> = self
            .declared_types()
            .into_iter()
            .filter(|t| *t != CellType::inactive())
            .collect();
        let names: Vec<&str> = types.iter().map(|t| t.as_str()).collect();
        let slots = distinct_palette_slots(&names, self.view.palette.len());
        for (ty, slot) in types.iter().zip(slots) {
            if let Some(&c) = self.view.palette.get(slot) {
                self.view.colors.insert(ty.0, c);
            }
        }
    }

    /// Apply the `colors` map from a config file. Unparseable values are
    /// reported in the status bar and the automatic colour stays.
    pub(in crate::gui) fn apply_config_colors(
        &mut self,
        colors: &std::collections::BTreeMap<String, String>,
    ) {
        self.view.config_colors = colors.clone();
        for (name, hex) in colors {
            match parse_hex_color(hex) {
                Some(c) => self.set_color_for(&CellType::from(name.as_str()), c),
                None => self.set_status(format!(
                    "Config colour for '{name}' is not #rrggbb: {hex:?} (ignored)"
                )),
            }
        }
    }

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

/// Status message for `CellaApp::finish_config_load`: names the file, the
/// dimension, and whether it resumed mid-run. `resume` is what the user
/// asked for; `resumed_ok` is whether that actually happened (a resume
/// request can fall back to `initial` if the snapshot doesn't build, e.g. a
/// hand-edited file with mismatched lengths).
fn load_status_message(dim: &str, name: &str, resume: bool, resumed_ok: bool) -> String {
    if resumed_ok {
        format!("Loaded config ({dim}, resumed): {name}")
    } else if resume {
        format!("Loaded config ({dim}): {name} (snapshot invalid, started from initial)")
    } else {
        format!("Loaded config ({dim}): {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::actions::Action;
    use crate::gui::actions::SnapshotChoice;
    use crate::gui::sim::tests::test_app;
    use cella_lib::config::CellaConfig;
    use std::path::Path;

    fn status(app: &CellaApp) -> String {
        app.chrome.status_message.clone().unwrap_or_default()
    }

    /// A tiny 2D config file whose `snapshot.step` is `step` (no `snapshot`
    /// at all when `step == 0`), with a custom colour on "A", for testing
    /// the "resume or start over" load flow and colour round-trip.
    fn write_snapshot_config_2d(step: u64) -> std::path::PathBuf {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                None,
                None,
            )],
        };
        let mut init = vec![a; 4];
        init[0] = b;
        let mut g = Grid2D::new(2, 2, 2, init, rule);
        let initial = GridState::from_grid2d(&g);
        for _ in 0..step {
            g.step();
        }
        let mut colors = std::collections::BTreeMap::new();
        colors.insert("A".to_string(), "#112233".to_string());
        let cfg = config::CellaConfig::save_2d(&initial, &g, colors);
        // Tests run in parallel within one process, so `process::id()` alone
        // collides between the several tests that both use `step == 3`; a
        // nanosecond timestamp keeps every call's file distinct.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cella_snapshot_load_test_{}_{}_{}.json",
            std::process::id(),
            step,
            stamp
        ));
        cfg.to_file_pretty(&path).expect("write test config");
        path
    }

    /// Like `write_snapshot_config_2d`, but with cell 0's saved history one
    /// entry longer than `history_limit` (2) — a hand-edited-looking file
    /// that `build_grid2d_resumed` must refuse rather than overrun the SoA
    /// history buffer for.
    fn write_snapshot_config_2d_with_oversized_history() -> std::path::PathBuf {
        let a = CellType::from("A");
        let b = CellType::from("B");
        let rule = Rule2D {
            subrules: vec![Rule2DSubrule::new(
                a,
                b,
                0,
                CountOp::Gt,
                1,
                Neighborhood2D::Moore,
                b,
                None,
                None,
            )],
        };
        let mut init = vec![a; 4];
        init[0] = b;
        let mut g = Grid2D::new(2, 2, 2, init, rule);
        let initial = GridState::from_grid2d(&g);
        for _ in 0..3 {
            g.step();
        }
        let mut cfg = config::CellaConfig::save_2d(&initial, &g, Default::default());
        let config::CellaConfig::D2(c) = &mut cfg else {
            panic!("expected D2");
        };
        let snap = c.snapshot.as_mut().expect("saved past step 0 has a snapshot");
        snap.history[0].push("A".to_string());
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cella_snapshot_oversized_history_test_{}_{}.json",
            std::process::id(),
            stamp
        ));
        cfg.to_file_pretty(&path).expect("write test config");
        path
    }

    #[test]
    fn resolving_resume_falls_back_to_initial_when_the_snapshot_history_is_oversized() {
        let mut app = test_app();
        let path = write_snapshot_config_2d_with_oversized_history();
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            app.chrome.pending_snapshot_load.is_some(),
            "step > 0 still prompts even though the snapshot is unusable"
        );
        app.apply_action(Action::ResolveSnapshotLoad(SnapshotChoice::Resume));
        assert!(app.chrome.pending_snapshot_load.is_none());
        assert_eq!(
            app.current_step(),
            0,
            "build_grid2d_resumed refuses the oversized history, so this falls back to initial"
        );
        assert!(
            status(&app).contains("snapshot invalid"),
            "status should explain the fallback, got {:?}",
            status(&app)
        );
    }

    #[test]
    fn loading_a_step_n_config_sets_the_pending_prompt_without_swapping_the_grid() {
        let mut app = test_app();
        app.load_demo_life();
        let path = write_snapshot_config_2d(3);
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            app.chrome.pending_snapshot_load.is_some(),
            "a step > 0 snapshot should prompt"
        );
        assert_eq!(
            app.current_step(),
            0,
            "the Life demo is untouched until the user answers"
        );
    }

    #[test]
    fn a_step_0_config_loads_immediately_with_no_prompt() {
        let mut app = test_app();
        let path = write_snapshot_config_2d(0);
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(app.chrome.pending_snapshot_load.is_none());
        assert_eq!(app.current_step(), 0);
        assert!(matches!(app.scenario.dim, Some(Dim::D2)));
    }

    #[test]
    fn resolving_resume_gives_step_n_and_the_config_colours() {
        let mut app = test_app();
        let path = write_snapshot_config_2d(3);
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        app.apply_action(Action::ResolveSnapshotLoad(SnapshotChoice::Resume));
        assert!(app.chrome.pending_snapshot_load.is_none());
        assert_eq!(app.current_step(), 3);
        assert_eq!(
            app.color_of(&CellType::from("A")),
            egui::Color32::from_rgb(0x11, 0x22, 0x33)
        );
        // Reset still returns to the scenario's step 0, not step 3.
        app.reset_to_initial();
        assert_eq!(app.current_step(), 0);
    }

    #[test]
    fn resolving_initial_gives_step_0_and_the_config_colours() {
        let mut app = test_app();
        let path = write_snapshot_config_2d(3);
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        app.apply_action(Action::ResolveSnapshotLoad(SnapshotChoice::Initial));
        assert!(app.chrome.pending_snapshot_load.is_none());
        assert_eq!(app.current_step(), 0);
        assert_eq!(
            app.color_of(&CellType::from("A")),
            egui::Color32::from_rgb(0x11, 0x22, 0x33)
        );
    }

    #[test]
    fn cancel_leaves_the_current_scenario_untouched() {
        let mut app = test_app();
        app.load_demo_life();
        let before_dim = app.scenario.dim;
        let path = write_snapshot_config_2d(3);
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        app.apply_action(Action::ResolveSnapshotLoad(SnapshotChoice::Cancel));
        assert!(app.chrome.pending_snapshot_load.is_none());
        assert_eq!(before_dim, app.scenario.dim, "the Life demo is still loaded");
        assert_eq!(app.current_step(), 0);
    }

    #[test]
    fn load_config_from_path_loads_a_2d_config_and_names_it_in_the_status() {
        let mut app = test_app();
        app.load_config_from_path(Path::new("configs/life.json"));
        assert!(matches!(app.scenario.dim, Some(Dim::D2)));
        assert!(app.scenario.d2.is_some());
        assert!(
            status(&app).contains("life.json"),
            "status should name the file, got {:?}",
            status(&app)
        );
    }

    #[test]
    fn load_config_from_path_reports_a_missing_file_in_the_status() {
        let mut app = test_app();
        app.load_config_from_path(Path::new("configs/does_not_exist.json"));
        assert!(app.scenario.dim.is_none(), "nothing should be loaded");
        assert!(
            status(&app).starts_with("Failed to load config"),
            "got {:?}",
            status(&app)
        );
    }

    /// A config that parses fine but can't build a grid at all — every
    /// `build_grid1d`/`build_grid2d`/`_resumed` guard rejects it, so this is
    /// a silent case unless `finish_config_load` reports it: it used to
    /// return with nothing but the previous scenario left in place and no
    /// hint why. `history_limit: 300` is one way to trip it (the SoA
    /// head/count arrays are `u8`); a bad `initial` length, a `width *
    /// height` overflow, or a model that fails to attach are the others.
    #[test]
    fn a_config_with_an_invalid_grid_reports_failure_without_touching_the_scenario() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cella_bad_history_limit_test_{}_{}.json",
            std::process::id(),
            stamp
        ));
        std::fs::write(
            &path,
            r#"{"dim":"2d","width":2,"height":2,"history_limit":300,
                "initial":["A","A","A","A"],
                "rule":{"subrules":[]}}"#,
        )
        .unwrap();

        let mut app = test_app();
        app.load_demo_life();
        let before_dim = app.scenario.dim;
        let before_step = app.current_step();
        let before_inputs = (app.inputs.grid_width, app.inputs.grid_height);

        app.load_config_from_path(&path); // must not panic
        let _ = std::fs::remove_file(&path);

        assert!(
            status(&app).starts_with("Failed to load config"),
            "got {:?}",
            status(&app)
        );
        assert_eq!(
            app.scenario.dim, before_dim,
            "the previous scenario (the Life demo) is untouched"
        );
        assert_eq!(app.current_step(), before_step);
        assert_eq!(
            (app.inputs.grid_width, app.inputs.grid_height),
            before_inputs
        );
    }

    #[test]
    fn an_empty_dialog_result_explains_itself_in_the_status() {
        let mut app = test_app();
        app.report_no_file_chosen("config");
        let msg = status(&app);
        assert!(msg.starts_with("No config chosen"), "got {msg:?}");
        assert!(
            msg.contains("--config"),
            "should point at the workaround, got {msg:?}"
        );
        assert!(
            msg.contains("docs/app.md"),
            "should point at the docs, got {msg:?}"
        );
    }

    #[test]
    fn apply_startup_config_loads_the_given_file_instead_of_the_demo() {
        let mut app = test_app();
        app.apply_startup_config(Some(Path::new("configs/1d_rule30_center.json")));
        assert!(matches!(app.scenario.dim, Some(Dim::D1)));
        assert!(status(&app).contains("1d_rule30_center.json"));
    }

    #[test]
    fn apply_startup_config_keeps_the_life_demo_under_a_bad_path_and_says_why() {
        let mut app = test_app();
        app.apply_startup_config(Some(Path::new("configs/does_not_exist.json")));
        assert!(
            matches!(app.scenario.dim, Some(Dim::D2)),
            "Life demo should be on screen"
        );
        assert_eq!((app.inputs.grid_width, app.inputs.grid_height), (50, 30));
        assert!(
            status(&app).starts_with("Failed to load config"),
            "got {:?}",
            status(&app)
        );
    }

    #[test]
    fn loading_a_scenario_gives_each_declared_type_its_own_colour() {
        let mut app = test_app();
        app.load_demo_2d_three_state_cycle();
        let types: Vec<CellType> = app
            .declared_types()
            .into_iter()
            .filter(|t| *t != CellType::inactive())
            .collect();
        assert!(types.len() >= 3, "demo declares at least three types");
        let colours: Vec<_> = types.iter().map(|t| app.color_of(t)).collect();
        for i in 0..colours.len() {
            for j in (i + 1)..colours.len() {
                assert_ne!(
                    colours[i],
                    colours[j],
                    "{} and {} share a colour",
                    types[i].as_str(),
                    types[j].as_str()
                );
            }
        }
    }

    #[test]
    fn config_colours_override_the_automatic_ones() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cella_colour_test_{}.json", std::process::id()));
        std::fs::write(
            &path,
            r##"{"dim":"2d","width":2,"height":1,"history_limit":0,
                "initial":["Forest","Burning"],
                "rule":{"subrules":[{"current_type":"Forest","criteria_type":"Burning","count":1,"op":"gt","range":1,"neighborhood":"Moore","randomness":null,"output_type":"Burning"}]},
                "colors":{"Forest":"#2e8b57"}}"##,
        )
        .unwrap();
        let mut app = test_app();
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            matches!(app.scenario.dim, Some(Dim::D2)),
            "got {:?}",
            status(&app)
        );
        let forest = app.color_of(&CellType::from("Forest"));
        let burning = app.color_of(&CellType::from("Burning"));
        assert_eq!(
            forest,
            egui::Color32::from_rgb(0x2e, 0x8b, 0x57),
            "config colour wins"
        );
        assert_ne!(
            forest, burning,
            "the other type still gets a distinct automatic colour"
        );
    }

    #[test]
    fn the_wildfire_demo_loads_with_its_four_evocative_colours() {
        let mut app = test_app();
        app.load_config_from_path(Path::new("configs/2d_wildfire_demo.json"));
        let c = |n: &str| app.color_of(&CellType::from(n));
        assert_eq!(c("Forest"), egui::Color32::from_rgb(0x2e, 0x8b, 0x57));
        assert_eq!(c("Shrub"), egui::Color32::from_rgb(0x9a, 0xcd, 0x32));
        assert_eq!(c("Burning"), egui::Color32::from_rgb(0xff, 0x45, 0x00));
        assert_eq!(c("BurnedOut"), egui::Color32::from_rgb(0x6b, 0x6b, 0x6b));
    }

    #[test]
    fn apply_startup_config_falls_back_to_the_life_demo_when_none() {
        let mut app = test_app();
        app.apply_startup_config(None);
        assert!(matches!(app.scenario.dim, Some(Dim::D2)));
        assert_eq!((app.inputs.grid_width, app.inputs.grid_height), (50, 30));
    }

    /// A temp-dir path that parallel tests won't share.
    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("cella_gui_{label}_{}_{stamp}.json", std::process::id()))
    }

    #[test]
    fn resize_keeps_the_model_step_colours_and_resizes_the_reset_target() {
        let mut app = test_app();
        app.load_config_from_path(Path::new("configs/2d_wildfire_demo.json"));
        for _ in 0..10 {
            app.scenario.d2.as_mut().unwrap().step();
        }
        let colours = app.view.colors.clone();
        app.apply_action(Action::Resize { w: 70, h: 44 });
        let g = app.scenario.d2.as_ref().unwrap();
        assert_eq!((g.width, g.height, g.step), (70, 44, 10));
        assert!(g.model.is_some(), "the model survives the resize");
        assert_eq!(app.view.colors, colours);
        let reset = Grid2D::from_state(app.scenario.initial_state.as_ref().unwrap()).unwrap();
        assert_eq!((reset.width, reset.height, reset.step), (70, 44, 0));
        assert!(status(&app).contains("70"), "got {:?}", status(&app));
        // The fire keeps burning: the model still drives the grid.
        app.scenario.d2.as_mut().unwrap().step();
        let g = app.scenario.d2.as_ref().unwrap();
        assert!((0..g.width * g.height).any(|i| g.cell_type(i) != g.inactive));
    }

    #[test]
    fn reset_after_resize_returns_to_step_0_at_the_new_size() {
        let mut app = test_app();
        app.load_demo_life();
        app.scenario.d2.as_mut().unwrap().step();
        app.apply_action(Action::Resize { w: 20, h: 12 });
        app.reset_to_initial();
        let g = app.scenario.d2.as_ref().unwrap();
        assert_eq!((g.width, g.height, g.step), (20, 12, 0));
    }

    #[test]
    fn resize_1d_keeps_the_step_and_the_history_view() {
        let mut app = test_app();
        app.load_demo_1d_rule30();
        for _ in 0..3 {
            app.step_once(); // src/gui/sim.rs: steps and pushes a history_1d row
        }
        let rows = app.view.history_1d.len();
        let step = app.scenario.d1.as_ref().unwrap().step;
        app.apply_action(Action::Resize { w: 40, h: 1 });
        assert_eq!(app.scenario.d1.as_ref().unwrap().width, 40);
        assert_eq!(app.scenario.d1.as_ref().unwrap().step, step);
        assert_eq!(app.view.history_1d.len(), rows);
    }

    #[test]
    fn save_then_load_after_a_resize_round_trips() {
        let mut app = test_app();
        app.load_demo_life();
        app.step_once();
        app.step_once();
        app.apply_action(Action::Resize { w: 20, h: 12 });
        let colors = app.color_map_for_save();
        let initial = app.scenario.initial_state.clone().expect("reset target");
        let g = app.scenario.d2.as_ref().expect("2D grid loaded");
        let cfg = CellaConfig::save_2d(&initial, g, colors);
        let path = unique_temp_path("resize_round_trip");
        cfg.to_file_pretty(&path).expect("write test config");
        let mut again = test_app();
        again.load_config_from_path(&path);
        again.resolve_snapshot_load(SnapshotChoice::Resume);
        let _ = std::fs::remove_file(&path);
        let g = again.scenario.d2.as_ref().unwrap();
        assert_eq!((g.width, g.height, g.step), (20, 12, 2));
        again.reset_to_initial();
        let g = again.scenario.d2.as_ref().unwrap();
        assert_eq!((g.width, g.height, g.step), (20, 12, 0));
    }

    #[test]
    fn a_resize_the_library_refuses_reports_the_error_and_touches_nothing() {
        let mut app = test_app();
        app.load_demo_life();
        app.step_once();
        let g = app.scenario.d2.as_ref().unwrap();
        let dims_before = (g.width, g.height);
        let step_before = g.step;
        let initial_before =
            Grid2D::from_state(app.scenario.initial_state.as_ref().unwrap()).unwrap();
        let initial_dims_before = (initial_before.width, initial_before.height);
        let colours_before = app.view.colors.clone();
        // width * height overflows usize, so the library's `checked_cells`
        // refuses before touching anything: `ResizeError::TooLarge`.
        app.apply_action(Action::Resize { w: usize::MAX, h: 2 });
        assert!(
            status(&app).starts_with("Resize failed"),
            "got {:?}",
            status(&app)
        );
        let g = app.scenario.d2.as_ref().unwrap();
        assert_eq!((g.width, g.height), dims_before, "the live grid is untouched");
        assert_eq!(g.step, step_before);
        let initial_after =
            Grid2D::from_state(app.scenario.initial_state.as_ref().unwrap()).unwrap();
        assert_eq!(
            (initial_after.width, initial_after.height),
            initial_dims_before,
            "the Reset target is untouched"
        );
        assert_eq!(app.view.colors, colours_before);
    }

    #[test]
    fn a_1d_resize_the_library_refuses_reports_the_error_and_touches_nothing() {
        let mut app = test_app();
        app.load_demo_1d_rule30();
        app.step_once();
        let g = app.scenario.d1.as_ref().unwrap();
        // `Grid1D::resize`'s overflow check multiplies by `history_limit.max(1)`,
        // so this only overflows for `usize::MAX` width when history_limit > 1;
        // the Rule 30 demo uses 5, so this reaches `ResizeError::TooLarge`.
        assert!(
            g.history_limit > 1,
            "demo's history_limit changed; this test needs it > 1 to overflow"
        );
        let width_before = g.width;
        let step_before = g.step;
        let initial_before =
            Grid1D::from_state(app.scenario.initial_state.as_ref().unwrap()).unwrap();
        let initial_width_before = initial_before.width;
        let colours_before = app.view.colors.clone();
        app.apply_action(Action::Resize { w: usize::MAX, h: 1 });
        assert!(
            status(&app).starts_with("Resize failed"),
            "got {:?}",
            status(&app)
        );
        let g = app.scenario.d1.as_ref().unwrap();
        assert_eq!(g.width, width_before, "the live grid is untouched");
        assert_eq!(g.step, step_before);
        let initial_after =
            Grid1D::from_state(app.scenario.initial_state.as_ref().unwrap()).unwrap();
        assert_eq!(
            initial_after.width, initial_width_before,
            "the Reset target is untouched"
        );
        assert_eq!(app.view.colors, colours_before);
    }
}
