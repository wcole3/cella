use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use cella_lib::*;
use egui::{Color32, Context, Key};
use rfd::FileDialog;

use crate::demos::{build_1d_code_n, build_1d_rule30, build_2d_life, build_2d_three_state_cycle};

use super::export::{export_gif_1d, export_gif_2d};
use super::render::default_palette;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dim { D1, D2 }

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawMode { Cycle, Paint }

/// Run the native GUI application.
pub fn run_gui(size: Option<(f32, f32)>) -> eframe::Result<()> {
    // Configure the initial window via NativeOptions/ViewportBuilder.
    // If size is provided, use it; otherwise default to a 16:9 reasonable size.
    let (mut w, mut h) = if let Some((w, h)) = size {
        (w, h)
    } else {
        let aspect = 16.0 / 9.0;
        let w = 1280.0_f32; // default width
        let h = (w / aspect) as f32;
        (w, h)
    };
    // Clamp minimums for the initial inner size
    w = w.max(800.0);
    h = h.max(450.0);

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(egui::vec2(w, h))
        .with_min_inner_size(egui::vec2(800.0, 450.0))
        .with_title("Cella GUI");

    let options = eframe::NativeOptions { viewport, ..eframe::NativeOptions::default() };

    eframe::run_native(
        "Cella GUI",
        options,
        Box::new(|cc| Box::new(CellaApp::new(cc))),
    )
}

/// Main GUI application state and logic.
///
/// This struct owns the currently loaded grid (1D or 2D), UI state such as
/// play/pause, per-type colors, and render/export settings.
struct CellaApp {
    // Simulation state (either 1D or 2D)
    d1: Option<Grid1D>,
    d2: Option<Grid2D>,
    dim: Option<Dim>,

    // Snapshot for reset
    initial_state: Option<GridState>,

    // UI Controls
    playing: bool,
    refresh_ms: u64,
    last_tick: Instant,
    run_to_steps: u64,
    run_to_target: Option<u64>,

    // Rendering
    scale: usize, // pixel size per cell
    colors: HashMap<String, Color32>,
    palette: Vec<Color32>,
    // Inactive color is configurable (affects on-screen and export)
    inactive_color: Color32,
    // Grid overlay
    show_grid_lines: bool,
    grid_line_color: Color32,

    // 1D history rendering
    history_1d: Vec<Vec<CellType>>, // past lines from oldest->newest (excluding current)
    history_limit_1d: usize,
    // Minimum number of rows to allocate in the 1D viewport to avoid scrollbars overlapping content
    min_view_rows_1d: usize,

    // Drawing mode
    draw_mode: DrawMode,
    selected_draw_type: Option<CellType>,
    // Undo support: stack of edit batches; each batch is Vec<(index, previous_type)>
    undo_stack: Vec<Vec<(usize, CellType)>>,
    current_paint_batch: Option<Vec<(usize, CellType)>>,

    // Export
    export_steps: u32,
    export_fps: u32,

    // Custom 1D builder inputs
    custom_code_input: String,
    custom_n: u8,

    // UI text/font scaling
    font_scale: f32,
    base_text_styles: BTreeMap<egui::TextStyle, egui::FontId>,
}

impl CellaApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            d1: None,
            d2: None,
            dim: None,
            initial_state: None,
            playing: false,
            refresh_ms: 100,
            last_tick: Instant::now(),
            run_to_steps: 100,
            run_to_target: None,
            scale: 8,
            colors: HashMap::new(),
            palette: default_palette(),
            inactive_color: Color32::from_rgb(30, 30, 35),
            show_grid_lines: true,
            grid_line_color: Color32::from_rgb(60, 60, 70),
            history_1d: Vec::new(),
            history_limit_1d: 100,
            min_view_rows_1d: 3,
            draw_mode: DrawMode::Cycle,
            selected_draw_type: Some(CellType::inactive()),
            undo_stack: Vec::new(),
            current_paint_batch: None,
            export_steps: 300,
            export_fps: 12,
            custom_code_input: "30".into(),
            custom_n: 1,
            font_scale: 1.0,
            base_text_styles: cc.egui_ctx.style().text_styles.clone(),
        };
        // Start with a default 2D Life-like demo
        app.load_demo_life();
        app
    }

    #[inline]
    fn inactive_color(&self) -> Color32 { self.inactive_color }

    fn set_color_for(&mut self, ty: &CellType, color: Color32) {
        if ty.0 == INACTIVE { self.inactive_color = color; return; }
        self.colors.insert(ty.0.clone(), color);
    }

    fn color_of(&self, ty: &CellType) -> Color32 {
        if ty.0 == INACTIVE { return self.inactive_color(); }
        if let Some(&c) = self.colors.get(&ty.0) { return c; }
        // fallback: hash name into palette index deterministically (no mutation)
        let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
        let prime: u64 = 0x00000100000001B3; // FNV prime
        for &b in ty.0.as_bytes() { h ^= b as u64; h = h.wrapping_mul(prime); }
        let idx = (h as usize) % self.palette.len().max(1);
        self.palette.get(idx).copied().unwrap_or(Color32::LIGHT_BLUE)
    }

    /// Apply user font scaling to egui text styles.
    fn apply_font_scale(&self, ctx: &Context) {
        let mut style = (*ctx.style()).clone();
        let mut map = self.base_text_styles.clone();
        for (_ts, font) in map.iter_mut() {
            font.size = (font.size * self.font_scale).max(6.0);
        }
        style.text_styles = map;
        ctx.set_style(style);
    }

    /// Advance the automaton one step and maintain the 1D history buffer.
    fn step_once(&mut self) {
        match self.dim {
            Some(Dim::D1) => if let Some(g) = &mut self.d1 {
                // push current row to history before stepping
                let mut row: Vec<CellType> = Vec::with_capacity(g.width);
                for x in 0..g.width { row.push(g.cells[x].current.clone()); }
                self.history_1d.push(row);
                if self.history_1d.len() > self.history_limit_1d { let overflow = self.history_1d.len() - self.history_limit_1d; self.history_1d.drain(0..overflow); }
                g.step();
            },
            Some(Dim::D2) => if let Some(g) = &mut self.d2 { g.step(); },
            None => {}
        }
    }

    /// Current step number from the loaded grid, or 0 when none loaded.
    fn current_step(&self) -> u64 {
        match self.dim {
            Some(Dim::D1) => self.d1.as_ref().map(|g| g.step).unwrap_or(0),
            Some(Dim::D2) => self.d2.as_ref().map(|g| g.step).unwrap_or(0),
            None => 0
        }
    }

    /// Collect the distinct types currently visible in the grid.
    fn collect_types(&mut self) -> Vec<CellType> {
        let mut set: HashSet<String> = HashSet::new();
        let mut result: Vec<CellType> = Vec::new();
        match self.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.d1 {
                    for c in &g.cells { if set.insert(c.current.0.clone()) { result.push(c.current.clone()); } }
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.d2 {
                    for c in &g.cells { if set.insert(c.current.0.clone()) { result.push(c.current.clone()); } }
                }
            }
            None => {}
        }
        result
    }

    /// Render the grid into a ColorImage respecting user colors and grid overlay.
    fn render_image(&mut self, _ctx: &Context) -> Option<egui::ColorImage> {
        // populate color map for current types to avoid mutable borrow during render
        let _ = self.collect_types();
        match self.dim {
            Some(Dim::D1) => {
                let g = self.d1.as_ref()?;
                let w = g.width.max(1);
                let total_rows = self.history_1d.len() + 1; // history + current
                let visible_rows = total_rows.max(self.min_view_rows_1d.max(1));
                let mut img = egui::ColorImage::new([w * self.scale, visible_rows * self.scale], self.inactive_color());
                // draw history rows
                for (row_i, row) in self.history_1d.iter().enumerate() {
                    let ww = w.min(row.len());
                    for x in 0..ww {
                        let col = self.color_of(&row[x]);
                        for dy in 0..self.scale {
                            for dx in 0..self.scale {
                                let px = x * self.scale + dx;
                                let py = row_i * self.scale + dy;
                                img[(px, py)] = col;
                            }
                        }
                    }
                }
                // draw current row at y = history_len (leaving padding at bottom)
                let current_y = self.history_1d.len();
                for x in 0..w {
                    let col = self.color_of(&g.cells[x].current);
                    for dy in 0..self.scale {
                        for dx in 0..self.scale {
                            let px = x * self.scale + dx;
                            let py = current_y * self.scale + dy;
                            img[(px, py)] = col;
                        }
                    }
                }
                // overlay grid lines
                if self.show_grid_lines {
                    let width_px = w * self.scale;
                    let height_px = visible_rows * self.scale;
                    let gc = self.grid_line_color;
                    for x in (0..width_px).step_by(self.scale) {
                        for y in 0..height_px { img[(x, y)] = gc; }
                    }
                    for y in (0..height_px).step_by(self.scale) {
                        for x in 0..width_px { img[(x, y)] = gc; }
                    }
                }
                Some(img)
            }
            Some(Dim::D2) => {
                let g = self.d2.as_ref()?;
                let w = g.width.max(1);
                let h = g.height.max(1);
                let mut img = egui::ColorImage::new([w * self.scale, h * self.scale], self.inactive_color());
                for y in 0..h {
                    for x in 0..w {
                        let idx = y * w + x;
                        let col = self.color_of(&g.cells[idx].current);
                        for dy in 0..self.scale {
                            for dx in 0..self.scale {
                                let px = x * self.scale + dx;
                                let py = y * self.scale + dy;
                                img[(px, py)] = col;
                            }
                        }
                    }
                }
                if self.show_grid_lines {
                    let width_px = w * self.scale;
                    let height_px = h * self.scale;
                    let gc = self.grid_line_color;
                    for x in (0..width_px).step_by(self.scale) {
                        for y in 0..height_px { img[(x, y)] = gc; }
                    }
                    for y in (0..height_px).step_by(self.scale) {
                        for x in 0..width_px { img[(x, y)] = gc; }
                    }
                }
                Some(img)
            }
            None => None,
        }
    }

    /// Build the top toolbar: play/pause, step, run-to, scale, export/save/reset.
    fn ui_top_controls(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        ui.horizontal(|ui| {
            if ui.button(if self.playing { "Pause" } else { "Play" }).clicked() {
                self.playing = !self.playing;
                self.last_tick = Instant::now();
            }
            if ui.button("Step").clicked() { self.step_once(); }
            ui.add(egui::DragValue::new(&mut self.refresh_ms).clamp_range(10..=2000).suffix(" ms"));
            ui.label("Refresh");
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.run_to_steps).clamp_range(1..=1_000_000).suffix(" steps"));
            if ui.button("Run to +N").clicked() {
                self.run_to_target = Some(self.current_step().saturating_add(self.run_to_steps));
                self.playing = true; // ensure stepping
            }
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.scale).clamp_range(1..=32).suffix(" px"));
            ui.label("Scale");
            ui.separator();
            if ui.button("Export GIF...").clicked() { self.export_gif_dialog(); }
            if ui.button("Save Final State").clicked() { self.save_final_state(); }
            if ui.button("Reset").clicked() { self.reset_to_initial(); }
        });
    }

    /// Build dataset/scenario controls (load config/demos and related settings).
    fn ui_dataset_controls(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Load/Select Scenario", |ui| {
            ui.horizontal(|ui| {
                if ui.button("Load Config JSON...").clicked() { self.load_config_dialog(); }
                if ui.button("Demo: Life (2D)").clicked() { self.load_demo_life(); }
                if ui.button("Demo: 1D Rule 30").clicked() { self.load_demo_1d_rule30(); }
                if ui.button("Demo: 1D n=2").clicked() { self.load_demo_1d_n2(); }
                if ui.button("Demo: 2D three-state").clicked() { self.load_demo_2d_three_state_cycle(); }
            });
            ui.separator();
            ui.label("Custom 1D (Wolfram code + n):");
            ui.horizontal(|ui| {
                ui.label("code:");
                ui.text_edit_singleline(&mut self.custom_code_input);
                ui.label("n:");
                ui.add(egui::DragValue::new(&mut self.custom_n).clamp_range(1..=8));
                if ui.button("Build").clicked() { self.load_demo_1d_custom_from_inputs(); }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_grid_lines, "Grid lines");
                let mut col = self.grid_line_color;
                if ui.color_edit_button_srgba(&mut col).changed() { self.grid_line_color = col; }
            });
            ui.horizontal(|ui| {
                ui.label("1D history limit:");
                ui.add(egui::DragValue::new(&mut self.history_limit_1d).clamp_range(1..=10_000));
            });
        });
    }

    /// Build the color editor panel, including the Inactive color.
    fn ui_colors(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Colors", |ui| {
            // Inactive color editor (always visible)
            let mut inact = self.inactive_color;
            ui.horizontal(|ui| {
                ui.label("Inactive:");
                if ui.color_edit_button_srgba(&mut inact).changed() { self.inactive_color = inact; }
            });
            ui.separator();
            // Dynamic colors for active types
            let tys = self.collect_types();
            for ty in tys {
                if ty.0 == INACTIVE { continue; }
                let mut col = self.color_of(&ty);
                let label = format!("{}", ty.0);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    self.set_color_for(&ty, col);
                }
                ui.label(label);
            }
        });
    }

    /// Play loop with fixed refresh step cadence; also handles "Run to +N".
    fn tick_play(&mut self) {
        // Fixed refresh: step at most once per refresh interval under play.
        let now = Instant::now();
        let interval = Duration::from_millis(self.refresh_ms);
        if self.playing && now.duration_since(self.last_tick) >= interval {
            self.last_tick = now;
            self.step_once();
        }
        if let Some(target) = self.run_to_target {
            while self.current_step() < target {
                // cap multiple steps per frame to avoid UI lockup
                for _ in 0..100 {
                    if self.current_step() >= target { break; }
                    self.step_once();
                }
                break; // let UI render
            }
            if self.current_step() >= target {
                self.run_to_target = None;
                self.playing = false;
            }
        }
    }

    // ----- Scenario loading -----
    fn load_demo_life(&mut self) {
        let (w,h,hist) = (50usize, 30usize, 5usize);
        self.d1 = None; self.dim = Some(Dim::D2);
        self.d2 = Some(build_2d_life(w,h,hist));
        self.initial_state = self.d2.as_ref().map(GridState::from_grid2d);
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        self.colors.clear();
        self.update_selected_draw_type_default();
    }

    fn load_demo_1d_rule30(&mut self) {
        let width = 201usize; let hist = 5usize;
        self.d2 = None; self.dim = Some(Dim::D1);
        self.d1 = Some(build_1d_rule30(width, hist));
        if let Some(g) = &self.d1 { self.initial_state = Some(GridState::from_grid1d(g)); }
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        self.colors.clear();
        self.update_selected_draw_type_default();
    }

    fn load_demo_1d_n2(&mut self) {
        let code: u128 = 0xAAAAAAAA;
        let width = 201usize; let hist = 5usize;
        self.d2 = None; self.dim = Some(Dim::D1);
        self.d1 = Some(build_1d_code_n(code, 2, width, hist).expect("n2 builder should validate"));
        if let Some(g) = &self.d1 { self.initial_state = Some(GridState::from_grid1d(g)); }
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        self.colors.clear();
        self.update_selected_draw_type_default()
    }

    fn load_demo_2d_three_state_cycle(&mut self) {
        let (w,h,hist) = (48usize, 27usize, 3usize);
        self.d1 = None; self.dim = Some(Dim::D2);
        self.d2 = Some(build_2d_three_state_cycle(w,h,hist));
        self.initial_state = self.d2.as_ref().map(GridState::from_grid2d);
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        self.colors.clear();
        self.update_selected_draw_type_default();
    }

    fn load_demo_1d_custom_from_inputs(&mut self) {
        let wolfram_code: u128 = self.custom_code_input.trim().parse().unwrap_or(30);
        let n: u8 = if self.custom_n == 0 { 1 } else { self.custom_n };
        let width = 201usize; let hist = 5usize;
        if let Ok(grid) = build_1d_code_n(wolfram_code, n, width, hist) {
            self.d2 = None; self.dim = Some(Dim::D1);
            self.d1 = Some(grid);
            if let Some(g) = &self.d1 { self.initial_state = Some(GridState::from_grid1d(g)); }
            self.history_1d.clear();
            self.undo_stack.clear();
            self.current_paint_batch = None;
            self.colors.clear();
            self.update_selected_draw_type_default();
        }
    }

    fn load_config_dialog(&mut self) {
        if let Some(path) = FileDialog::new().add_filter("json", &["json"]).pick_file() {
            match config::CellaConfig::from_file(&path) {
                Ok(cfg) => match cfg {
                    config::CellaConfig::D1(_) => {
                        if let Some(g) = cfg.build_grid1d() {
                            self.dim = Some(Dim::D1); self.d1 = Some(g); self.d2 = None;
                            if let Some(gr) = &self.d1 { self.initial_state = Some(GridState::from_grid1d(gr)); }
                            self.history_1d.clear(); self.undo_stack.clear(); self.current_paint_batch = None; self.colors.clear();
                            self.update_selected_draw_type_default();
                        }
                    }
                    config::CellaConfig::D2(_) => {
                        if let Some(g) = cfg.build_grid2d() {
                            self.dim = Some(Dim::D2); self.d2 = Some(g); self.d1 = None;
                            if let Some(gr) = &self.d2 { self.initial_state = Some(GridState::from_grid2d(gr)); }
                            self.history_1d.clear(); self.undo_stack.clear(); self.current_paint_batch = None; self.colors.clear();
                            self.update_selected_draw_type_default();
                        }
                    }
                },
                Err(e) => { eprintln!("Failed to load config: {}", e); }
            }
        }
    }

    // ----- Save/Export -----
    fn save_final_state(&mut self) {
        let state = match self.dim {
            Some(Dim::D1) => self.d1.as_ref().map(GridState::from_grid1d),
            Some(Dim::D2) => self.d2.as_ref().map(GridState::from_grid2d),
            None => None,
        };
        if let Some(st) = state {
            if let Some(path) = FileDialog::new().set_file_name("snapshot.json").save_file() {
                let json = serde_json::to_string_pretty(&st).unwrap();
                let _ = std::fs::write(path, json);
            }
        }
    }

    /// Export an animated GIF using the current color settings (including Inactive).
    fn export_gif_dialog(&mut self) {
        if let Some(path) = FileDialog::new().set_file_name("cella.gif").save_file() {
            let steps = self.export_steps.max(1);
            let fps = self.export_fps.max(1);
            match self.dim {
                Some(Dim::D1) => if let Some(g) = &self.d1 { let mut clone = g.clone(); let _ = export_gif_1d(&mut clone, path.clone(), steps as usize, fps, self.scale as u16, &self.colors, &self.palette, self.inactive_color()); },
                Some(Dim::D2) => if let Some(g) = &self.d2 { let mut clone = g.clone(); let _ = export_gif_2d(&mut clone, path.clone(), steps as usize, fps, self.scale as u16, &self.colors, &self.palette, self.inactive_color()); },
                None => {}
            }
        }
    }

    /// Reset the current grid to its initial snapshot captured on load.
    fn reset_to_initial(&mut self) {
        self.playing = false;
        self.run_to_target = None;
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        if let Some(st) = &self.initial_state {
            match st {
                GridState::D1 { .. } => {
                    if let Some(g) = Grid1D::from_state(st) {
                        self.dim = Some(Dim::D1);
                        self.d1 = Some(g);
                        self.d2 = None;
                    }
                }
                GridState::D2 { .. } => {
                    if let Some(g) = Grid2D::from_state(st) {
                        self.dim = Some(Dim::D2);
                        self.d2 = Some(g);
                        self.d1 = None;
                    }
                }
            }
        }
        // Update default draw type after resetting
        self.update_selected_draw_type_default();
    }

    /// Update default draw type to the first non-Inactive type in the grid, else Inactive.
    fn update_selected_draw_type_default(&mut self) {
        let mut pick: Option<CellType> = None;
        match self.dim {
            Some(Dim::D1) => if let Some(g) = &self.d1 {
                for c in &g.cells { if c.current.0 != INACTIVE { pick = Some(c.current.clone()); break; } }
            },
            Some(Dim::D2) => if let Some(g) = &self.d2 {
                for c in &g.cells { if c.current.0 != INACTIVE { pick = Some(c.current.clone()); break; } }
            },
            None => {}
        }
        self.selected_draw_type = Some(pick.unwrap_or_else(CellType::inactive));
    }

    /// Show a collapsible panel with per-type statistics (current and peak counts).
    fn ui_statistics(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Statistics", |ui| {
            let mut entries: Vec<(String, u64, u64)> = Vec::new();
            match self.dim {
                Some(Dim::D1) => if let Some(g) = &self.d1 {
                    let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                    for k in g.counts_current.keys() { keys.insert(k.clone()); }
                    for k in g.peak_counts.keys() { keys.insert(k.clone()); }
                    for k in keys {
                        let cur = *g.counts_current.get(&k).unwrap_or(&0);
                        let peak = *g.peak_counts.get(&k).unwrap_or(&0);
                        entries.push((k, cur, peak));
                    }
                },
                Some(Dim::D2) => if let Some(g) = &self.d2 {
                    let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                    for k in g.counts_current.keys() { keys.insert(k.clone()); }
                    for k in g.peak_counts.keys() { keys.insert(k.clone()); }
                    for k in keys {
                        let cur = *g.counts_current.get(&k).unwrap_or(&0);
                        let peak = *g.peak_counts.get(&k).unwrap_or(&0);
                        entries.push((k, cur, peak));
                    }
                },
                None => {}
            }
            // Sort with Inactive first, then by name
            entries.sort_by(|a, b| {
                let ai = (a.0 != INACTIVE) as u8;
                let bi = (b.0 != INACTIVE) as u8;
                ai.cmp(&bi).then_with(|| a.0.cmp(&b.0))
            });
            let mut total: u64 = 0;
            for (_n, c, _p) in &entries { total = total.saturating_add(*c); }
            ui.label(format!("Total cells: {}", total));
            for (name, cur, peak) in entries {
                ui.horizontal(|ui| {
                    ui.label(format!("{:>10}: {} (peak {})", name, cur, peak));
                });
            }
        });
    }
}

impl eframe::App for CellaApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.apply_font_scale(ctx);
        egui::TopBottomPanel::top("top_controls").show(ctx, |ui| {
            self.ui_top_controls(ui, ctx);
        });
        egui::SidePanel::left("left_controls").default_width(260.0).show(ctx, |ui| {
            self.ui_dataset_controls(ui);
            ui.separator();
            ui.collapsing("UI Settings", |ui| {
                ui.horizontal(|ui| {
                    if ui.button("A-").clicked() { self.font_scale = (self.font_scale - 0.1).max(0.5); }
                    if ui.button("A+").clicked() { self.font_scale = (self.font_scale + 0.1).min(3.0); }
                    ui.label(format!("Font: {:.0}%", self.font_scale * 100.0));
                });
                ui.add(egui::Slider::new(&mut self.font_scale, 0.5..=3.0).text("Font scale"));
            });
            ui.separator();
            ui.collapsing("Editing", |ui| {
                ui.horizontal(|ui| {
                    let is_cycle = matches!(self.draw_mode, DrawMode::Cycle);
                    if ui.radio(is_cycle, "Cycle").clicked() { self.draw_mode = DrawMode::Cycle; }
                    let is_paint = matches!(self.draw_mode, DrawMode::Paint);
                    if ui.radio(is_paint, "Paint").clicked() { self.draw_mode = DrawMode::Paint; }
                });
                ui.horizontal(|ui| {
                    ui.label("Paint type:");
                    // Build a type list including Inactive
                    let mut names: Vec<String> = vec![INACTIVE.to_string()];
                    for ty in self.collect_types() {
                        if ty.0 != INACTIVE { names.push(ty.0.clone()); }
                    }
                    let current_name = self.selected_draw_type.as_ref().map(|t| t.0.clone()).unwrap_or_else(|| INACTIVE.to_string());
                    let mut sel = current_name.clone();
                    egui::ComboBox::from_label("")
                        .selected_text(sel.clone())
                        .show_ui(ui, |ui| {
                            for n in &names { ui.selectable_value(&mut sel, n.clone(), n); }
                        });
                    if sel != current_name { self.selected_draw_type = Some(CellType(sel)); }
                });
                ui.label("Hold and drag on the grid while paused to paint.");
            });
            ui.separator();
            ui.collapsing("Export", |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.export_steps).clamp_range(1..=10_000));
                    ui.label("steps");
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.export_fps).clamp_range(1..=60));
                    ui.label("fps");
                });
            });
            ui.separator();
            self.ui_colors(ui);
            ui.separator();
            self.ui_statistics(ui);
        });

        egui::TopBottomPanel::bottom("bottom_status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Step: {}", self.current_step()));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Hotkeys: Ctrl+Z undo last edit when paused
            if !self.playing {
                ui.input(|i| {
                    if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(Key::Z) {
                        if let Some(batch) = self.undo_stack.pop() {
                            match self.dim {
                                Some(Dim::D1) => if let Some(g) = &mut self.d1 { for (idx, prev) in batch { if idx < g.width { g.cells[idx].transition(&prev); } } },
                                Some(Dim::D2) => if let Some(g) = &mut self.d2 { for (idx, prev) in batch { if idx < g.cells.len() { g.cells[idx].transition(&prev); } } },
                                None => {}
                            }
                        }
                    }
                });
            }
            egui::ScrollArea::both().drag_to_scroll(false).show(ui, |ui| {
                if let Some(img) = self.render_image(ctx) {
                    let tex = ui.ctx().load_texture(
                        "grid_tex",
                        egui::ImageData::Color(img.into()),
                        egui::TextureOptions::NEAREST,
                    );
                    let size = tex.size_vec2();
                    let response = ui.add(egui::Image::new(&tex).fit_to_exact_size(size).sense(egui::Sense::click_and_drag()));

                    // Zoom with MouseWheel when hovered
                    if response.hovered() {
                        ui.input(|i| {
                            if i.raw_scroll_delta.y > 0.0 { self.scale = (self.scale + 1).min(32); }
                            else if i.raw_scroll_delta.y < 0.0 { self.scale = self.scale.saturating_sub(1).max(1); }
                        });
                    }

                    if response.dragged() {
                        // Right mouse drag to pan the scroll area (so left is free for painting)
                        let (right_down, delta) = ui.input(|i| (i.pointer.secondary_down(), i.pointer.delta()));
                        if right_down {
                            if delta.x != 0.0 || delta.y != 0.0 { ui.scroll_with_delta(delta); }
                        }
                    }

                    // Painting mode: click/drag to set cells when paused
                    if !self.playing && matches!(self.draw_mode, DrawMode::Paint) {
                        let is_down = ui.input(|i| i.pointer.primary_down());
                        if is_down {
                            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                                if response.rect.contains(pos) {
                                    let local = pos - response.rect.min;
                                    let px = local.x.max(0.0) as usize;
                                    let py = local.y.max(0.0) as usize;
                                    let cell_x = px / self.scale.max(1);
                                    let cell_y = py / self.scale.max(1);
                                    let paint_ty = self.selected_draw_type.clone().unwrap_or_else(CellType::inactive);
                                    match self.dim {
                                        Some(Dim::D1) => {
                                            if let Some(g) = &mut self.d1 {
                                                let total_rows = self.history_1d.len() + 1;
                                                if total_rows > 0 && cell_y == total_rows - 1 && cell_x < g.width {
                                                    let idx = cell_x;
                                                    let prev = g.cells[idx].current.clone();
                                                    if prev != paint_ty {
                                                        if self.current_paint_batch.is_none() { self.current_paint_batch = Some(Vec::new()); }
                                                        if let Some(batch) = &mut self.current_paint_batch {
                                                            if !batch.iter().any(|(j, _)| *j == idx) { batch.push((idx, prev.clone())); }
                                                        }
                                                        g.cells[idx].transition(&paint_ty);
                                                    }
                                                }
                                            }
                                        }
                                        Some(Dim::D2) => {
                                            if let Some(g) = &mut self.d2 {
                                                if cell_x < g.width && cell_y < g.height {
                                                    let idx = cell_y * g.width + cell_x;
                                                    let prev = g.cells[idx].current.clone();
                                                    if prev != paint_ty {
                                                        if self.current_paint_batch.is_none() { self.current_paint_batch = Some(Vec::new()); }
                                                        if let Some(batch) = &mut self.current_paint_batch {
                                                            if !batch.iter().any(|(j, _)| *j == idx) { batch.push((idx, prev.clone())); }
                                                        }
                                                        g.cells[idx].transition(&paint_ty);
                                                    }
                                                }
                                            }
                                        }
                                        None => {}
                                    }
                                }
                            }
                        } else {
                            if let Some(batch) = self.current_paint_batch.take() { if !batch.is_empty() { self.undo_stack.push(batch); } }
                        }
                    }

                    // Cycle mode: Click to edit when paused
                    if response.clicked() && !self.playing && matches!(self.draw_mode, DrawMode::Cycle) {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let local = pos - response.rect.min;
                            let px = local.x.max(0.0) as usize;
                            let py = local.y.max(0.0) as usize;
                            let cell_x = px / self.scale.max(1);
                            let cell_y = py / self.scale.max(1);
                            match self.dim {
                                Some(Dim::D1) => {
                                    if let Some(g) = &mut self.d1 {
                                        let total_rows = self.history_1d.len() + 1;
                                        if total_rows > 0 && cell_y == total_rows - 1 && cell_x < g.width {
                                            let current = g.cells[cell_x].current.clone();
                                            // Build type list locally to avoid borrowing self
                                            let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                                            for c in &g.cells { set.insert(c.current.0.clone()); }
                                            let mut names: Vec<String> = Vec::new();
                                            names.push(INACTIVE.to_string());
                                            for n in set { if n != INACTIVE { names.push(n); } }
                                            let tys: Vec<CellType> = names.iter().map(|s| CellType(s.clone())).collect();
                                            let mut idx = tys.iter().position(|t| t == &current).unwrap_or(0);
                                            idx = (idx + 1) % tys.len();
                                            let next = tys[idx].clone();
                                            // push undo batch of one cell
                                            self.undo_stack.push(vec![(cell_x, current.clone())]);
                                            g.cells[cell_x].transition(&next);
                                        }
                                    }
                                }
                                Some(Dim::D2) => {
                                    if let Some(g) = &mut self.d2 {
                                        if cell_x < g.width && cell_y < g.height {
                                            let i = cell_y * g.width + cell_x;
                                            let current = g.cells[i].current.clone();
                                            // Build type list locally to avoid borrowing self
                                            let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                                            for c in &g.cells { set.insert(c.current.0.clone()); }
                                            let mut names: Vec<String> = Vec::new();
                                            names.push(INACTIVE.to_string());
                                            for n in set { if n != INACTIVE { names.push(n); } }
                                            let tys: Vec<CellType> = names.iter().map(|s| CellType(s.clone())).collect();
                                            let mut idx = tys.iter().position(|t| t == &current).unwrap_or(0);
                                            idx = (idx + 1) % tys.len();
                                            let next = tys[idx].clone();
                                            self.undo_stack.push(vec![(i, current.clone())]);
                                            g.cells[i].transition(&next);
                                        }
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                } else {
                    ui.label("No grid loaded.");
                }
            });
        });

        self.tick_play();
        ctx.request_repaint_after(Duration::from_millis(10));
    }
}
