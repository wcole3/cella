use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cella_lib::*;
use egui::{Color32, Context, Key};
use egui_plot::{Plot, Line, PlotPoints, Legend};
use rfd::FileDialog;

use crate::demos::{build_1d_code_n, build_1d_rule30, build_2d_life, build_2d_three_state_cycle, build_2d_straightline};

use super::export::{export_gif_1d, export_gif_2d};
use super::render::default_palette;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dim { D1, D2 }

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawMode { Cycle, Paint }

// ---------------- Rule Editor Models ----------------
#[derive(Clone, Debug)]
struct Rule1DSubruleEdit {
    current: String,
    criteria: String,
    wolfram_code: String,
    n: u8,
    randomness_enabled: bool,
    randomness_value: f64,
    output: String,
}

#[derive(Clone, Debug)]
struct Rule1DEdit { subrules: Vec<Rule1DSubruleEdit> }

impl Rule1DEdit {
    fn from_rule(rule: &Rule1D) -> Self {
        let subs = rule.subrules.iter().map(|s| Rule1DSubruleEdit {
            current: s.current_type.0.clone(),
            criteria: s.criteria_type.0.clone(),
            wolfram_code: s.wolfram_code.to_string(),
            n: s.n,
            randomness_enabled: s.randomness.is_some(),
            randomness_value: s.randomness.unwrap_or(0.0),
            output: s.output_type.0.clone(),
        }).collect();
        Self { subrules: subs }
    }
    fn to_rule(&self) -> Result<Rule1D, String> {
        let mut subs: Vec<Rule1DSubrule> = Vec::new();
        for s in &self.subrules {
            let code = s.wolfram_code.trim().parse::<u128>().map_err(|e| format!("wolfram_code parse error: {}", e))?;
            let randomness = if s.randomness_enabled { Some(s.randomness_value) } else { None };
            let sub = Rule1DSubrule {
                current_type: CellType(s.current.clone()),
                criteria_type: CellType(s.criteria.clone()),
                wolfram_code: code,
                n: s.n,
                randomness,
                output_type: CellType(s.output.clone()),
            };
            if let Err(e) = sub.validate() { return Err(format!("validation error: {}", e)); }
            subs.push(sub);
        }
        Ok(Rule1D { subrules: subs })
    }
}

#[derive(Clone, Debug)]
struct Rule2DSubruleEdit {
    current: String,
    criteria: String,
    count: u32,
    op: CountOp,
    limit_enabled: bool,
    limit_value: u32,
    range: u8,
    neighborhood: Neighborhood2D,
    randomness_enabled: bool,
    randomness_value: f64,
    output: String,
}

#[derive(Clone, Debug)]
struct Rule2DEdit { subrules: Vec<Rule2DSubruleEdit> }

impl Rule2DEdit {
    fn from_rule(rule: &Rule2D) -> Self {
        let subs = rule.subrules.iter().map(|s| Rule2DSubruleEdit {
            current: s.current_type.0.clone(),
            criteria: s.criteria_type.0.clone(),
            count: s.count,
            op: s.op,
            limit_enabled: s.limit.is_some(),
            limit_value: s.limit.unwrap_or(0),
            range: s.range,
            neighborhood: s.neighborhood,
            randomness_enabled: s.randomness.is_some(),
            randomness_value: s.randomness.unwrap_or(0.0),
            output: s.output_type.0.clone(),
        }).collect();
        Self { subrules: subs }
    }
    fn to_rule(&self) -> Result<Rule2D, String> {
        let mut subs: Vec<Rule2DSubrule> = Vec::new();
        for s in &self.subrules {
            let randomness = if s.randomness_enabled { Some(s.randomness_value) } else { None };
            let limit = if s.limit_enabled { Some(s.limit_value) } else { None };
            let sub = Rule2DSubrule {
                current_type: CellType(s.current.clone()),
                criteria_type: CellType(s.criteria.clone()),
                count: s.count,
                op: s.op,
                limit,
                range: s.range,
                neighborhood: s.neighborhood,
                randomness,
                output_type: CellType(s.output.clone()),
            };
            if let Err(e) = sub.validate() { return Err(format!("validation error: {}", e)); }
            subs.push(sub);
        }
        Ok(Rule2D { subrules: subs })
    }
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

    // Status bar message
    status_message: Option<String>,

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
    export_1d_with_history: bool,
    export_total: usize,
    export_progress: Option<Arc<AtomicUsize>>,
    export_join: Option<std::thread::JoinHandle<Result<(), String>>>,
    export_message: Option<String>,

    // Custom 1D builder inputs
    custom_code_input: String,
    custom_n: u8,

    // UI text/font scaling
    font_scale: f32,
    base_text_styles: BTreeMap<egui::TextStyle, egui::FontId>,

    // Statistics history for per-type counts (sliding window)
    stats_history: BTreeMap<String, Vec<(u64, u64)>>,
    stats_show: BTreeMap<String, bool>,
    stats_window_len: usize,

    // Rule editor state
    rule_edit_1d: Option<Rule1DEdit>,
    rule_edit_2d: Option<Rule2DEdit>,
    rule_error_msg: Option<String>,
    // Extra declared types added via UI (beyond those seen in rules/initial grid)
    custom_types: std::collections::BTreeSet<String>,

    // Transient input for adding a new type/state
    new_type_name: String,

    // UI: visibility of the right-side Rule Editor panel
    show_rule_editor: bool,
}

impl CellaApp {
    fn set_status<S: Into<String>>(&mut self, msg: S) { self.status_message = Some(msg.into()); }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            d1: None,
            d2: None,
            dim: None,
            status_message: None,
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
            export_1d_with_history: false,
            export_total: 0,
            export_progress: None,
            export_join: None,
            export_message: None,
            custom_code_input: "30".into(),
            custom_n: 1,
            font_scale: 1.0,
            base_text_styles: cc.egui_ctx.style().text_styles.clone(),
            stats_history: BTreeMap::new(),
            stats_show: BTreeMap::new(),
            stats_window_len: 300,
            // Rule editor defaults
            rule_edit_1d: None,
            rule_edit_2d: None,
            rule_error_msg: None,
            custom_types: std::collections::BTreeSet::new(),
            new_type_name: String::new(),
            show_rule_editor: true,
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
        // record stats after a successful step
        self.stats_record_step();
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

    /// Collect all declared types for the current scenario (from rules/config),
    /// including Inactive, regardless of whether they are currently present on the grid.
    fn declared_types(&self) -> Vec<CellType> {
        use std::collections::BTreeSet;
        let mut set: BTreeSet<String> = BTreeSet::new();
        set.insert(INACTIVE.to_string());
        match self.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.d1 {
                    for s in &g.rule.subrules {
                        set.insert(s.current_type.0.clone());
                        set.insert(s.criteria_type.0.clone());
                        set.insert(s.output_type.0.clone());
                    }
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.d2 {
                    for s in &g.rule.subrules {
                        set.insert(s.current_type.0.clone());
                        set.insert(s.criteria_type.0.clone());
                        set.insert(s.output_type.0.clone());
                    }
                }
            }
            None => {}
        }
        // Include any extra types added via the editor
        for t in &self.custom_types { set.insert(t.clone()); }
        // Order with Inactive first, then alphabetical for readability
        let mut names: Vec<String> = set.into_iter().collect();
        names.sort();
        names.sort_by(|a, b| (a != INACTIVE).cmp(&(b != INACTIVE)));
        names.into_iter().map(CellType).collect()
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
                if self.playing { self.set_status("Playing"); } else { self.set_status("Paused"); }
            }
            if ui.button("Step").clicked() { self.step_once(); self.set_status(format!("Stepped to {}", self.current_step())); }
            ui.add(egui::DragValue::new(&mut self.refresh_ms).clamp_range(10..=2000).suffix(" ms"));
            ui.label("Refresh");
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.run_to_steps).clamp_range(1..=1_000_000).suffix(" steps"));
            if ui.button("Run to +N").clicked() {
                let target = self.current_step().saturating_add(self.run_to_steps);
                self.run_to_target = Some(target);
                self.playing = true; // ensure stepping
                self.set_status(format!("Running to {}", target));
            }
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.scale).clamp_range(1..=32).suffix(" px"));
            ui.label("Scale");
            ui.separator();
            let exporting = self.export_join.is_some();
            let export_btn = ui.add_enabled(!exporting, egui::Button::new("Export GIF..."));
            if export_btn.clicked() { self.export_gif_dialog(); }
            if exporting { ui.label("Exporting..."); }
            if ui.button("Save Final State").clicked() { self.save_final_state(); }
            if ui.button("Reset").clicked() { self.reset_to_initial(); }
            ui.separator();
            let toggle = if self.show_rule_editor { "Hide Rule Editor" } else { "Show Rule Editor" };
            if ui.button(toggle).clicked() { self.show_rule_editor = !self.show_rule_editor; }
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
                if ui.button("Demo: 2D straightline").clicked() { self.load_demo_2d_straightline(); }
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
            ui.separator();
            // Rule editor moved to the right panel; see right-side Rule Editor panel.
        });
    }

    /// Build the color editor panel, including the Inactive color.
    /// Rule editor residing in the Scenario panel. Allows adding types and fully editing rules.
    fn ui_rule_editor(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Rule editor", |ui| {
            // Manage known types
            ui.label("Types/states available to rules:").on_hover_text("Declare the distinct cell states used by your rules. 'Inactive' is reserved and always present.");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_type_name)
                    .on_hover_text("Enter a new state name (e.g., Alive, Dead, A, B). Avoid using 'Inactive'.");
                if ui.button("Add type")
                    .on_hover_text("Add the typed state so it can be used in rules and colored in the viewport.")
                    .clicked() {
                    let name = self.new_type_name.trim();
                    if !name.is_empty() && name != INACTIVE {
                        self.custom_types.insert(name.to_string());
                        self.set_status(format!("Added type '{}'", name));
                        // set a default color if desired (optional; fallback hash works)
                        self.new_type_name.clear();
                    }
                }
            });
            // Show current list
            let mut names: Vec<String> = self.declared_types().into_iter().map(|t| t.0).collect();
            names.sort(); names.sort_by(|a,b| (a != INACTIVE).cmp(&(b != INACTIVE)));
            ui.horizontal_wrapped(|ui| {
                for n in &names { ui.label(egui::RichText::new(n.clone()).monospace()); }
            });
            ui.separator();

            if let Some(dim) = self.dim {
                match dim {
                    Dim::D1 => {
                        // Ensure editor model exists
                        if self.rule_edit_1d.is_none() {
                            if let Some(g) = &self.d1 { self.rule_edit_1d = Some(Rule1DEdit::from_rule(&g.rule)); }
                        }
                        if let Some(edit) = &mut self.rule_edit_1d {
                            // Subrules list (scrollable)
                            let mut remove_idx: Option<usize> = None;
                            let mut move_up_idx: Option<usize> = None;
                            let mut move_down_idx: Option<usize> = None;
                            ui.set_min_height(240.0);
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                for i in 0..edit.subrules.len() {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("Subrule #{}", i+1));
                                        if ui.button("Remove").clicked() { remove_idx = Some(i); }
                                    });
                                    let ty_names = names.clone();
                                    let sub = &mut edit.subrules[i];
                                    // current
                                    ui.horizontal(|ui| {
                                        ui.label("current:")
                                            .on_hover_text("Center cell must currently be this state for the subrule to apply.");
                                        let mut sel = sub.current.clone();
                                        egui::ComboBox::from_id_source(format!("d1_cur_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| {
                                                for n in &ty_names { ui.selectable_value(&mut sel, n.clone(), n); }
                                            });
                                        if sel != sub.current { sub.current = sel; }
                                    });
                                    // criteria
                                    ui.horizontal(|ui| {
                                        ui.label("criteria:")
                                            .on_hover_text("Neighbor cells equal to this state are treated as 1s in the Wolfram pattern; others are 0s.");
                                        let mut sel = sub.criteria.clone();
                                        egui::ComboBox::from_id_source(format!("d1_crit_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| {
                                                for n in &ty_names { ui.selectable_value(&mut sel, n.clone(), n); }
                                            });
                                        if sel != sub.criteria { sub.criteria = sel; }
                                    });
                                    // output
                                    ui.horizontal(|ui| {
                                        ui.label("output:")
                                            .on_hover_text("The new state to set when this subrule matches.");
                                        let mut sel = sub.output.clone();
                                        egui::ComboBox::from_id_source(format!("d1_out_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| {
                                                for n in &ty_names { ui.selectable_value(&mut sel, n.clone(), n); }
                                            });
                                        if sel != sub.output { sub.output = sel; }
                                    });
                                    // code and n
                                    ui.horizontal(|ui| {
                                        ui.label("wolfram code:")
                                            .on_hover_text("Bitmask for patterns over a (2n+1) window of neighbors: 1 = match triggers. Indexing uses a binary window where neighbors equal to 'criteria' are 1.");
                                        ui.text_edit_singleline(&mut sub.wolfram_code)
                                            .on_hover_text("Enter a non-negative integer (u128). For n=1 there are 2^(3)=8 patterns; for larger n the number grows quickly.");
                                        ui.label("n:")
                                            .on_hover_text("Neighborhood radius (>=1). The window size is 2n+1 around the center cell.");
                                        ui.add(egui::DragValue::new(&mut sub.n).clamp_range(1..=8))
                                            .on_hover_text("Radius n between 1 and 8.");
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Up").clicked() { move_up_idx = Some(i); }
                                        if ui.button("Down").clicked() { move_down_idx = Some(i); }
                                    });
                                    // randomness
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut sub.randomness_enabled, "randomness")
                                            .on_hover_text("Optional stochasticity: when enabled, the match will only apply with probability 1 - p.");
                                        if sub.randomness_enabled {
                                            ui.add(egui::Slider::new(&mut sub.randomness_value, 0.0..=1.0).text("p").fixed_decimals(3))
                                                .on_hover_text("Probability p to cancel the match (so the rule applies with probability 1 - p).");
                                        }
                                    });
                                });
                            }
                            });
                            if let Some(i) = move_up_idx { if i > 0 { edit.subrules.swap(i, i - 1); } }
                            if let Some(i) = move_down_idx { if i + 1 < edit.subrules.len() { edit.subrules.swap(i, i + 1); } }
                            if let Some(idx) = remove_idx { edit.subrules.remove(idx); }
                            if ui.button("Add subrule").clicked() {
                                edit.subrules.push(Rule1DSubruleEdit{ current: INACTIVE.to_string(), criteria: INACTIVE.to_string(), wolfram_code: "0".into(), n: 1, randomness_enabled: false, randomness_value: 0.0, output: INACTIVE.to_string()});
                            }
                            if ui.button("Apply to grid").clicked() {
                                match edit.to_rule() {
                                    Ok(rule) => {
                                        if let Some(g) = &mut self.d1 { g.rule = rule; }
                                        self.rule_error_msg = None;
                                        self.refresh_rule_editor_from_current();
                                        self.set_status("Applied 1D rule");
                                    }
                                    Err(e) => { self.rule_error_msg = Some(e.clone()); self.set_status(format!("Rule error: {}", e)); }
                                }
                            }
                        } else {
                            ui.label("No 1D grid loaded.");
                        }
                    }
                    Dim::D2 => {
                        if self.rule_edit_2d.is_none() {
                            if let Some(g) = &self.d2 { self.rule_edit_2d = Some(Rule2DEdit::from_rule(&g.rule)); }
                        }
                        if let Some(edit) = &mut self.rule_edit_2d {
                            let mut remove_idx: Option<usize> = None;
                            let mut move_up_idx: Option<usize> = None;
                            let mut move_down_idx: Option<usize> = None;
                            ui.set_min_height(240.0);
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                for i in 0..edit.subrules.len() {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("Subrule #{}", i+1));
                                        if ui.button("Remove").clicked() { remove_idx = Some(i); }
                                    });
                                    let ty_names = names.clone();
                                    let sub = &mut edit.subrules[i];
                                    // current
                                    ui.horizontal(|ui| {
                                        ui.label("current:")
                                            .on_hover_text("Center cell must currently be this state for the subrule to apply.");
                                        let mut sel = sub.current.clone();
                                        egui::ComboBox::from_id_source(format!("d2_cur_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| { for n in &ty_names { ui.selectable_value(&mut sel, n.clone(), n); } });
                                        if sel != sub.current { sub.current = sel; }
                                    });
                                    // criteria
                                    ui.horizontal(|ui| {
                                        ui.label("criteria:")
                                            .on_hover_text("Neighbor cells of this state are counted within the chosen neighborhood.");
                                        let mut sel = sub.criteria.clone();
                                        egui::ComboBox::from_id_source(format!("d2_crit_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| { for n in &ty_names { ui.selectable_value(&mut sel, n.clone(), n); } });
                                        if sel != sub.criteria { sub.criteria = sel; }
                                    });
                                    // output
                                    ui.horizontal(|ui| {
                                        ui.label("output:")
                                            .on_hover_text("The new state to set when this subrule matches.");
                                        let mut sel = sub.output.clone();
                                        egui::ComboBox::from_id_source(format!("d2_out_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| { for n in &ty_names { ui.selectable_value(&mut sel, n.clone(), n); } });
                                        if sel != sub.output { sub.output = sel; }
                                    });
                                    // neighborhood modifiers
                                    ui.horizontal(|ui| {
                                        ui.label("count:")
                                            .on_hover_text("Baseline neighbor count for comparison. See 'op' for how it is used.");
                                        ui.add(egui::DragValue::new(&mut sub.count).clamp_range(0..=99))
                                            .on_hover_text("Set the baseline count between 0 and 99.");
                                        ui.label("op:")
                                            .on_hover_text("Comparison: gt means >= count, lt means <= count, eq means exactly count. With a limit, you can specify a range.");
                                        let mut op = sub.op; 
                                        egui::ComboBox::from_id_source(format!("d2_op_{}", i))
                                            .selected_text(match op { CountOp::Lt=>"lt", CountOp::Gt=>"gt", CountOp::Eq=>"eq" })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut op, CountOp::Lt, "lt");
                                                ui.selectable_value(&mut op, CountOp::Gt, "gt");
                                                ui.selectable_value(&mut op, CountOp::Eq, "eq");
                                            });
                                        sub.op = op;
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut sub.limit_enabled, "limit")
                                            .on_hover_text("Optional second bound to create a range: with op=gt, checks count in [count..=limit]; with op=lt, checks count in [limit..=count].");
                                        if sub.limit_enabled { 
                                            ui.add(egui::DragValue::new(&mut sub.limit_value).clamp_range(0..=99))
                                                .on_hover_text("Inclusive bound for the range comparison."); 
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("range n:")
                                            .on_hover_text("Neighborhood range (>=1). The square window is (2n+1)^2, filtered by the chosen neighborhood type.");
                                        ui.add(egui::DragValue::new(&mut sub.range).clamp_range(1..=8))
                                            .on_hover_text("Set range n between 1 and 8.");
                                        ui.label("neighborhood:")
                                            .on_hover_ui(|ui| {
                                                ui.label("Neighborhood shape around the center (@):");
                                                let diag = CellaApp::neighborhood_ascii(sub.range, sub.neighborhood);
                                                ui.monospace(diag);
                                                ui.small("Legend: @ center, # counted neighbor, . outside");
                                                ui.separator();
                                                ui.label("Moore = square; VonNeumann = Manhattan distance; Langdon = diagonals; StraightLine = cardinal lines only");
                                            });
                                        let mut nb = sub.neighborhood;
                                        egui::ComboBox::from_id_source(format!("d2_nh_{}", i))
                                            .selected_text(match nb { Neighborhood2D::Moore=>"Moore", Neighborhood2D::VonNeumann=>"VonNeumann", Neighborhood2D::Langdon=>"Langdon", Neighborhood2D::StraightLine=>"StraightLine" })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut nb, Neighborhood2D::Moore, "Moore");
                                                ui.selectable_value(&mut nb, Neighborhood2D::VonNeumann, "VonNeumann");
                                                ui.selectable_value(&mut nb, Neighborhood2D::Langdon, "Langdon");
                                                ui.selectable_value(&mut nb, Neighborhood2D::StraightLine, "StraightLine");
                                            });
                                        sub.neighborhood = nb;
                                    });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut sub.randomness_enabled, "randomness")
                                            .on_hover_text("Optional stochasticity: when enabled, the match will only apply with probability 1 - p.");
                                        if sub.randomness_enabled {
                                            ui.add(egui::Slider::new(&mut sub.randomness_value, 0.0..=1.0).text("p").fixed_decimals(3))
                                                .on_hover_text("Probability p to cancel the match (so the rule applies with probability 1 - p).");
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Up").clicked() { move_up_idx = Some(i); }
                                        if ui.button("Down").clicked() { move_down_idx = Some(i); }
                                    });
                                });
                            }
                            });
                            if let Some(i) = move_up_idx { if i > 0 { edit.subrules.swap(i, i - 1); } }
                            if let Some(i) = move_down_idx { if i + 1 < edit.subrules.len() { edit.subrules.swap(i, i + 1); } }
                            if let Some(idx) = remove_idx { edit.subrules.remove(idx); }
                            if ui.button("Add subrule").clicked() {
                                edit.subrules.push(Rule2DSubruleEdit{ current: INACTIVE.to_string(), criteria: INACTIVE.to_string(), count: 0, op: CountOp::Gt, limit_enabled: false, limit_value: 0, range: 1, neighborhood: Neighborhood2D::Moore, randomness_enabled: false, randomness_value: 0.0, output: INACTIVE.to_string() });
                            }
                            if ui.button("Apply to grid").clicked() {
                                match edit.to_rule() {
                                    Ok(rule) => {
                                        if let Some(g) = &mut self.d2 { g.rule = rule; }
                                        self.rule_error_msg = None;
                                        self.refresh_rule_editor_from_current();
                                        self.set_status("Applied 2D rule");
                                    }
                                    Err(e) => { self.rule_error_msg = Some(e.clone()); self.set_status(format!("Rule error: {}", e)); }
                                }
                            }
                        } else {
                            ui.label("No 2D grid loaded.");
                        }
                    }
                }
            } else {
                ui.label("No grid loaded.");
            }

            if let Some(err) = &self.rule_error_msg { ui.colored_label(egui::Color32::RED, format!("Rule error: {}", err)); }
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
            // Colors for all declared types (from rules/config), not just those currently present
            let tys = self.declared_types();
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
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
        self.set_status("Loaded demo: Life (2D)");
    }

    fn load_demo_1d_rule30(&mut self) {
        let width = 201usize; let hist = 5usize;
        self.d2 = None; self.dim = Some(Dim::D1);
        self.d1 = Some(build_1d_rule30(width, hist));
        self.set_status("Loaded demo: 1D Rule 30");
        if let Some(g) = &self.d1 { self.initial_state = Some(GridState::from_grid1d(g)); }
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        self.colors.clear();
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
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
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }

    fn load_demo_2d_three_state_cycle(&mut self) {
        let (w,h,hist) = (48usize, 27usize, 3usize);
        self.d1 = None; self.dim = Some(Dim::D2);
        self.d2 = Some(build_2d_three_state_cycle(w,h,hist));
        self.set_status("Loaded demo: 2D three-state cycle");
        self.initial_state = self.d2.as_ref().map(GridState::from_grid2d);
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        self.colors.clear();
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }

    fn load_demo_2d_straightline(&mut self) {
        let (w,h,hist) = (48usize, 27usize, 3usize);
        self.d1 = None; self.dim = Some(Dim::D2);
        self.d2 = Some(build_2d_straightline(w,h,hist));
        self.set_status("Loaded demo: 2D StraightLine");
        self.initial_state = self.d2.as_ref().map(GridState::from_grid2d);
        self.history_1d.clear();
        self.undo_stack.clear();
        self.current_paint_batch = None;
        self.colors.clear();
        self.update_selected_draw_type_default();
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
    }

    fn load_demo_1d_custom_from_inputs(&mut self) {
        let wolfram_code: u128 = self.custom_code_input.trim().parse().unwrap_or(30);
        let n: u8 = if self.custom_n == 0 { 1 } else { self.custom_n };
        let width = 201usize; let hist = 5usize;
        if let Ok(grid) = build_1d_code_n(wolfram_code, n, width, hist) {
            self.d2 = None; self.dim = Some(Dim::D1);
            self.d1 = Some(grid);
            self.set_status(format!("Loaded custom 1D: code={}, n={}", wolfram_code, n));
            if let Some(g) = &self.d1 { self.initial_state = Some(GridState::from_grid1d(g)); }
            self.history_1d.clear();
            self.undo_stack.clear();
            self.current_paint_batch = None;
            self.colors.clear();
            self.update_selected_draw_type_default();
            self.stats_clear_and_init();
            self.refresh_rule_editor_from_current();
        }
    }

    fn load_config_dialog(&mut self) {
        if let Some(path) = FileDialog::new().add_filter("json", &["json"]).pick_file() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("config.json").to_string();
            match config::CellaConfig::from_file(&path) {
                Ok(cfg) => match cfg {
                    config::CellaConfig::D1(_) => {
                        if let Some(g) = cfg.build_grid1d() {
                            self.dim = Some(Dim::D1); self.d1 = Some(g); self.d2 = None;
                            self.set_status(format!("Loaded config (1D): {}", name));
                            if let Some(gr) = &self.d1 { self.initial_state = Some(GridState::from_grid1d(gr)); }
                            self.history_1d.clear(); self.undo_stack.clear(); self.current_paint_batch = None; self.colors.clear();
                            self.update_selected_draw_type_default();
                            self.stats_clear_and_init();
                        }
                    }
                    config::CellaConfig::D2(_) => {
                        if let Some(g) = cfg.build_grid2d() {
                            self.dim = Some(Dim::D2); self.d2 = Some(g); self.d1 = None;
                            self.set_status(format!("Loaded config (2D): {}", name));
                            if let Some(gr) = &self.d2 { self.initial_state = Some(GridState::from_grid2d(gr)); }
                            self.history_1d.clear(); self.undo_stack.clear(); self.current_paint_batch = None; self.colors.clear();
                            self.update_selected_draw_type_default();
                            self.stats_clear_and_init();
                        }
                    }
                },
                Err(e) => { eprintln!("Failed to load config: {}", e); }
            }
            // After loading any config, sync the rule editor
            self.refresh_rule_editor_from_current();
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
    /// Runs the export in a background thread and shows a progress bar; optionally
    /// continues stepping the live grid while exporting based on `export_live_update`.
    fn export_gif_dialog(&mut self) {
        if self.export_join.is_some() { return; }
        if let Some(path) = FileDialog::new().add_filter("gif", &["gif"]).set_file_name("cella.gif").save_file() {
            let steps = self.export_steps.max(1) as usize;
            let fps = self.export_fps.max(1);
            let scale = self.scale as u16;
            let colors = self.colors.clone();
            let palette = self.palette.clone();
            let inactive = self.inactive_color();
            let progress = Arc::new(AtomicUsize::new(0));
            self.export_total = steps;
            self.export_progress = Some(progress.clone());
            self.set_status(format!("Exporting GIF: {} frames @ {} fps", steps, fps));
            match self.dim {
                Some(Dim::D1) => if let Some(g) = &self.d1 {
                    let mut grid_clone = g.clone();
                    let history_opt = if self.export_1d_with_history { Some(self.history_limit_1d) } else { None };
                    let path2 = path.clone();
                    let handle = std::thread::spawn(move || {
                        export_gif_1d(&mut grid_clone, path2, steps, fps, scale, &colors, &palette, inactive, history_opt, Some(&progress))
                            .map_err(|e| e.to_string())
                    });
                    self.export_join = Some(handle);
                },
                Some(Dim::D2) => if let Some(g) = &self.d2 {
                    let mut grid_clone = g.clone();
                    let path2 = path.clone();
                    let handle = std::thread::spawn(move || {
                        export_gif_2d(&mut grid_clone, path2, steps, fps, scale, &colors, &palette, inactive, Some(&progress))
                            .map_err(|e| e.to_string())
                    });
                    self.export_join = Some(handle);
                },
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
        self.stats_clear_and_init();
        self.refresh_rule_editor_from_current();
        self.set_status("Reset to initial state");
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

    /// Internal: clear and initialize statistics history/toggles from current grid.
    fn stats_clear_and_init(&mut self) {
        self.stats_history.clear();
        self.stats_show.clear();
        let (counts, step) = match self.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.d1 { (g.counts_current.clone(), g.step) } else { return; }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.d2 { (g.counts_current.clone(), g.step) } else { return; }
            }
            None => return,
        };
        // Build ordered keys: Inactive first, then by name
        let mut keys: Vec<String> = counts.keys().cloned().collect();
        if !keys.iter().any(|k| k == INACTIVE) { keys.push(INACTIVE.to_string()); }
        keys.sort();
        keys.sort_by(|a, b| (a != INACTIVE).cmp(&(b != INACTIVE)));
        // Default visibility: first 9 active types (Inactive off by default)
        let mut shown_left = 9usize;
        for k in keys {
            let show = if k == INACTIVE { false } else if shown_left > 0 { shown_left -= 1; true } else { false };
            self.stats_show.insert(k.clone(), show);
            let c = *counts.get(&k).unwrap_or(&0);
            self.stats_history.insert(k, vec![(step, c)]);
        }
    }

    /// Sync the rule editor model from the currently loaded grid and clear errors.
    fn refresh_rule_editor_from_current(&mut self) {
        self.rule_error_msg = None;
        match self.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.d1 {
                    self.rule_edit_1d = Some(Rule1DEdit::from_rule(&g.rule));
                    self.rule_edit_2d = None;
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.d2 {
                    self.rule_edit_2d = Some(Rule2DEdit::from_rule(&g.rule));
                    self.rule_edit_1d = None;
                }
            }
            None => { self.rule_edit_1d = None; self.rule_edit_2d = None; }
        }
    }

    /// Internal: after stepping, append counts for each known type and cap window.
    fn stats_record_step(&mut self) {
        let (counts, step) = match self.dim {
            Some(Dim::D1) => { if let Some(g) = &self.d1 { (g.counts_current.clone(), g.step) } else { return; } }
            Some(Dim::D2) => { if let Some(g) = &self.d2 { (g.counts_current.clone(), g.step) } else { return; } }
            None => return,
        };
        // Ensure entries for any newly seen types (default hidden, including Inactive)
        for (k, _) in counts.iter() {
            if !self.stats_history.contains_key(k) {
                self.stats_history.insert(k.clone(), Vec::new());
                self.stats_show.entry(k.clone()).or_insert(false);
            }
        }
        if !self.stats_history.contains_key(INACTIVE) {
            self.stats_history.insert(INACTIVE.to_string(), Vec::new());
            self.stats_show.entry(INACTIVE.to_string()).or_insert(false);
        }
        // Union of keys
        let mut keys: Vec<String> = self.stats_history.keys().cloned().collect();
        keys.sort();
        keys.sort_by(|a, b| (a != INACTIVE).cmp(&(b != INACTIVE)));
        for k in keys {
            let v = counts.get(&k).copied().unwrap_or(0);
            if let Some(list) = self.stats_history.get_mut(&k) {
                list.push((step, v));
                if list.len() > self.stats_window_len { let drop = list.len() - self.stats_window_len; list.drain(0..drop); }
            }
        }
    }

    /// Show a collapsible panel with per-type statistics (current and peak counts),
    /// and a running history chart (fixed-size window) similar to Task Manager.
    fn ui_statistics(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Statistics", |ui| {
            // Current/peak table
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
            entries.sort_by(|a, b| {
                let ai = (a.0 != INACTIVE) as u8;
                let bi = (b.0 != INACTIVE) as u8;
                ai.cmp(&bi).then_with(|| a.0.cmp(&b.0))
            });
            let mut total: u64 = 0;
            for (_n, c, _p) in &entries { total = total.saturating_add(*c); }
            ui.label(format!("Total cells: {}", total));
            for (name, cur, peak) in &entries {
                ui.horizontal(|ui| {
                    ui.label(format!("{:>10}: {} (peak {})", name, cur, peak));
                });
            }
            ui.separator();

            // Visibility toggles
            ui.label("Series shown in graph:");
            let mut names: Vec<String> = self.stats_history.keys().cloned().collect();
            //names.sort();
            names.sort_by(|a, b| (a != INACTIVE).cmp(&(b != INACTIVE)));
            ui.horizontal_wrapped(|ui| {
                for n in &names {
                    let mut show = *self.stats_show.get(n).unwrap_or(&false);
                    let color = self.color_of(&CellType(n.clone()));
                    let label = egui::RichText::new(n.clone()).color(color);
                    if ui.checkbox(&mut show, label).changed() {
                        self.stats_show.insert(n.clone(), show);
                    }
                }
            });

            // Line plot of the last N samples per selected series
            let plot = Plot::new("stats_plot").legend(Legend::default());
            plot.show(ui, |plot_ui| {
                for n in names {
                    if !self.stats_show.get(&n).copied().unwrap_or(false) { continue; }
                    if let Some(list) = self.stats_history.get(&n) {
                        if list.is_empty() { continue; }
                        let pts: PlotPoints = list.iter().map(|(s, v)| [*s as f64, *v as f64]).collect::<Vec<_>>().into();
                        let color = self.color_of(&CellType(n.clone()));
                        let line = Line::new(pts).name(n.clone()).color(color);
                        plot_ui.line(line);
                    }
                }
            });
        });
    }

    fn neighborhood_ascii(range: u8, kind: Neighborhood2D) -> String {
        let n = range as i32;
        let mut out = String::new();
        let name = match kind {
            Neighborhood2D::Moore => "Moore",
            Neighborhood2D::VonNeumann => "VonNeumann",
            Neighborhood2D::Langdon => "Langdon",
            Neighborhood2D::StraightLine => "StraightLine",
        };
        out.push_str(&format!("{} (n={})\n", name, range));
        for dy in -n..=n {
            for dx in -n..=n {
                if dx == 0 && dy == 0 {
                    out.push('@');
                } else {
                    let inside = match kind {
                        Neighborhood2D::Moore => dx.abs() <= n && dy.abs() <= n,
                        Neighborhood2D::VonNeumann => dx.abs() + dy.abs() <= n,
                        Neighborhood2D::Langdon => dx.abs() == dy.abs() && dx.abs() <= n,
                        Neighborhood2D::StraightLine => (dx == 0 && dy.abs() <= n) || (dy == 0 && dx.abs() <= n),
                    };
                    out.push(if inside { '#' } else { '.' });
                }
            }
            if dy != n { out.push('\n'); }
        }
        out
    }
}

impl eframe::App for CellaApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.apply_font_scale(ctx);
        egui::TopBottomPanel::top("top_controls").show(ctx, |ui| {
            self.ui_top_controls(ui, ctx);
        });
        egui::SidePanel::left("left_controls").default_width(260.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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
                        // Build a type list from rule/config declared types (not just currently present)
                        let mut names: Vec<String> = self.declared_types().into_iter().map(|t| t.0).collect();
                        // Ensure ordering with Inactive first
                        names.sort();
                        names.sort_by(|a, b| (a != INACTIVE).cmp(&(b != INACTIVE)));
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
                    ui.collapsing("Options", |ui| {
                        ui.horizontal(|ui| {
                            let mut flag = self.export_1d_with_history;
                            if ui.checkbox(&mut flag, "1D GIF: include vertical history").changed() {
                                self.export_1d_with_history = flag;
                            }
                        });
                        ui.small("Applies to 1D GIF export; height limited by 1D history limit.");
                    });
                    ui.separator();
                    if let Some(p) = &self.export_progress {
                        let done = p.load(Ordering::Relaxed) as u32;
                        let total = self.export_total.max(1) as u32;
                        let frac = (done as f32) / (total as f32);
                        ui.add(egui::ProgressBar::new(frac).text(format!("Exporting: {} / {}", done, total)));
                    }
                    if let Some(msg) = &self.export_message { ui.label(msg.clone()); }
                });
                ui.separator();
                self.ui_colors(ui);
                ui.separator();
                self.ui_statistics(ui);
            });
        });

        // Right-side Rule Editor panel (resizable, can be hidden via toggle)
        if self.show_rule_editor {
            egui::SidePanel::right("right_rule_editor")
                .resizable(true)
                .min_width(220.0)
                .default_width(340.0)
                .show(ctx, |ui| {
                    ui.heading("Rule Editor");
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        self.ui_rule_editor(ui);
                    });
                });
        }

        egui::TopBottomPanel::bottom("bottom_status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Step: {}", self.current_step()));
                if let Some(msg) = &self.status_message {
                    ui.separator();
                    ui.label(egui::RichText::new(msg.clone()).italics());
                }
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

        // If an export thread is active, poll for completion and finalize
        if let Some(handle) = &self.export_join {
            if handle.is_finished() {
                if let Some(handle) = self.export_join.take() {
                    match handle.join().unwrap_or_else(|_| Err("export thread panicked".to_string())) {
                        Ok(()) => { self.export_message = Some("Export complete".into()); self.set_status("Export complete"); },
                        Err(e) => { let msg = format!("Export failed: {}", e); self.export_message = Some(msg.clone()); self.set_status(msg); },
                    }
                }
                self.export_progress = None;
                self.export_total = 0;
            }
        }

        self.tick_play();
        ctx.request_repaint_after(Duration::from_millis(10));
    }
}
