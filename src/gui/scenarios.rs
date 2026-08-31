//! Loading and reshaping what the simulation is running.
//!
//! Everything here answers "what grid are we looking at": the built-in demos,
//! loading a JSON config from disk, resizing the grid, and resetting back to
//! the state a scenario started in.

use std::path::Path;
use std::time::Duration;

use super::app::{CellaApp, Dim};
use super::render::{distinct_palette_slots, parse_hex_color};
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
        match FileDialog::new().add_filter("json", &["json"]).pick_file() {
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

    /// Load a config file, replacing the grid, the Reset snapshot, and the editor
    /// state. Reports success or failure in the status bar.
    pub(in crate::gui) fn load_config_from_path(&mut self, path: &Path) {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("config.json")
            .to_string();
        match config::CellaConfig::from_file(path) {
            Ok(cfg) => {
                match cfg {
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
                            self.reset_colors_for_scenario();
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
                            self.reset_colors_for_scenario();
                            self.update_selected_draw_type_default();
                            self.stats_clear_and_init();
                        }
                    }
                }
                self.apply_config_colors(cfg.colors());
            }
            Err(e) => {
                let msg = format!("Failed to load config: {}", e);
                eprintln!("{}", msg);
                self.set_status(msg);
            }
        }
        // After loading any config, sync the rule editor
        self.refresh_rule_editor_from_current();
    }

    /// Give every declared type its own colour for the scenario that was just
    /// loaded. Hashing names into eight palette slots collides easily (the
    /// wildfire demo had Forest and Burning both land on sky blue), so instead
    /// each type takes the next free slot, in the stable order `declared_types`
    /// returns. Explicit colours from a config are applied on top afterwards.
    pub(in crate::gui) fn reset_colors_for_scenario(&mut self) {
        self.view.colors.clear();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::sim::tests::test_app;
    use std::path::Path;

    fn status(app: &CellaApp) -> String {
        app.chrome.status_message.clone().unwrap_or_default()
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
}
