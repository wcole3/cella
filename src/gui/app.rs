//! Core GUI application built on [`eframe`] / [`egui`].
//!
//! [`CellaApp`] is the main application struct that implements `eframe::App`.
//! It manages simulation state, rule editing, grid rendering, playback
//! controls, statistics, drawing/painting, and GIF export.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::export::{export_gif_1d, export_gif_2d, GifExport};
use super::render::{color_for, default_palette};
use crate::demos::{build_1d_code_n, build_1d_rule30, build_2d_life, build_2d_straightline, build_2d_three_state_cycle};
use cella_lib::types::interner;
use cella_lib::*;
use egui::scroll_area::{DragScroll, ScrollSource};
use egui::{Color32, Context, Key, Shape, TextEdit};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use lasso2::Spur;
use rfd::FileDialog;

/// Accumulates grid cells into merged, same-colored rectangles for one frame.
///
/// See [`CellaApp::paint_grid_viewport`] for why the merging matters.
struct RowPainter<'a, F: Fn(CellType) -> Color32> {
    shapes: &'a mut Vec<Shape>,
    /// Linear-probed color memo. Scenarios have a handful of types, so a scan
    /// beats hashing and it avoids re-running the fallback FNV hash per cell.
    color_cache: Vec<(Spur, Color32)>,
    /// Maps a cell type to its color; memoized through `color_cache`.
    resolve: F,
    /// Top-left corner of the full (unclipped) grid in screen coordinates.
    origin: egui::Pos2,
    /// Pixels per cell.
    scale: f32,
    /// Background color; runs of this color are left unpainted.
    bg: Color32,
}

impl<F: Fn(CellType) -> Color32> RowPainter<'_, F> {
    #[inline]
    fn color_of(&mut self, ty: CellType) -> Color32 {
        if let Some(&(_, c)) = self.color_cache.iter().find(|(s, _)| *s == ty.0) { return c; }
        let c = (self.resolve)(ty);
        self.color_cache.push((ty.0, c));
        c
    }

    /// Emit merged runs of same-colored cells for the cells `xs` of grid row `row_y`.
    fn emit_row(&mut self, row_y: usize, xs: std::ops::Range<usize>, cell_at: impl Fn(usize) -> CellType) {
        if xs.is_empty() { return; }
        let (x_start, x_end) = (xs.start, xs.end);
        let y = self.origin.y + row_y as f32 * self.scale;
        let mut run_start = x_start;
        let mut run_color = self.color_of(cell_at(x_start));
        for x in (x_start + 1)..=x_end {
            // At `x_end` the sentinel forces the final run to be flushed.
            let col = if x < x_end { self.color_of(cell_at(x)) } else { run_color };
            if x == x_end || col != run_color {
                if run_color != self.bg {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(self.origin.x + run_start as f32 * self.scale, y),
                        egui::vec2((x - run_start) as f32 * self.scale, self.scale),
                    );
                    self.shapes.push(Shape::rect_filled(rect, 0.0, run_color));
                }
                run_start = x;
                run_color = col;
            }
        }
    }
}

/// Next type after `current` in `types`, wrapping around. Falls back to `Inactive`
/// when `types` is empty or `current` is not a declared type.
fn next_in_cycle(types: &[CellType], current: CellType) -> CellType {
    if types.is_empty() { return CellType::inactive(); }
    let idx = types.iter().position(|t| *t == current).unwrap_or(0);
    types[(idx + 1) % types.len()]
}

/// Sort type names so that `Inactive` comes first and the rest are alphabetical.
fn sort_types_inactive_first(names: &mut [CellType]) {
    names.sort_by(|a, b| {
        let (a, b) = (a.as_str(), b.as_str());
        (a != INACTIVE).cmp(&(b != INACTIVE)).then_with(|| a.cmp(b))
    });
}

/// Upper bound on simulation steps executed in a single frame while a
/// "Run to +N" target is pending, so the UI stays responsive.
const RUN_TO_STEPS_PER_FRAME: u32 = 100;

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
            current: s.current_type.as_str().to_string(),
            criteria: s.criteria_type.as_str().to_string(),
            wolfram_code: s.wolfram_code.to_string(),
            n: s.n,
            randomness_enabled: s.randomness.is_some(),
            randomness_value: s.randomness.unwrap_or(0.0),
            output: s.output_type.as_str().to_string(),
        }).collect();
        Self { subrules: subs }
    }
    fn to_rule(&self) -> Result<Rule1D, String> {
        let mut subs: Vec<Rule1DSubrule> = Vec::new();
        for s in &self.subrules {
            let code = s.wolfram_code.trim().parse::<u128>().map_err(|e| format!("wolfram_code parse error: {}", e))?;
            let randomness = if s.randomness_enabled { Some(s.randomness_value) } else { None };
            let sub = Rule1DSubrule {
                current_type: CellType::from(s.current.clone()),
                criteria_type: CellType::from(s.criteria.clone()),
                wolfram_code: code,
                n: s.n,
                randomness,
                output_type: CellType::from(s.output.clone()),
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
            current: s.current_type.as_str().to_string(),
            criteria: s.criteria_type.as_str().to_string(),
            count: s.count,
            op: s.op,
            limit_enabled: s.limit.is_some(),
            limit_value: s.limit.unwrap_or(0),
            range: s.range,
            neighborhood: s.neighborhood,
            randomness_enabled: s.randomness.is_some(),
            randomness_value: s.randomness.unwrap_or(0.0),
            output: s.output_type.as_str().to_string(),
        }).collect();
        Self { subrules: subs }
    }
    fn to_rule(&self) -> Result<Rule2D, String> {
        let mut subs: Vec<Rule2DSubrule> = Vec::new();
        for s in &self.subrules {
            let randomness = if s.randomness_enabled { Some(s.randomness_value) } else { None };
            let limit = if s.limit_enabled { Some(s.limit_value) } else { None };
            let sub = Rule2DSubrule::new(
                CellType::from(s.current.clone()),
                CellType::from(s.criteria.clone()),
                s.count,
                s.op,
                s.range,
                s.neighborhood,
                CellType::from(s.output.clone()),
                randomness,
                limit
            );
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
        let h = w / aspect;
        (w, h)
    };
    // Clamp minimums for the initial inner size
    w = w.max(800.0);
    h = h.max(450.0);

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(egui::vec2(w, h))
        .with_min_inner_size(egui::vec2(800.0, 450.0))
        .with_title("Cella GUI").with_resizable(true);

    let options = eframe::NativeOptions { viewport, ..eframe::NativeOptions::default() };

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
    /// Explicit per-type colors, keyed by the interned symbol so lookups during
    /// painting are a plain integer hash rather than a `String` allocation.
    colors: HashMap<Spur, Color32>,
    palette: Vec<Color32>,
    /// Scratch buffer for the shapes emitted by [`CellaApp::paint_grid_viewport`],
    /// reused every frame so painting does not reallocate.
    shape_buf: Vec<Shape>,
    // Inactive color is configurable (affects on-screen and export)
    inactive_color: Color32,
    // Grid overlay
    show_grid_lines: bool,
    grid_line_color: Color32,

    // 1D history rendering.
    //
    // This is deliberately separate from `Grid1D`'s own history: the library keeps a
    // short per-cell circular buffer (`history_limit`, typically a handful of entries)
    // that rules use for age/lookback, whereas the 1D viewport wants whole-row
    // snapshots going back hundreds of steps so it can draw the classic space-time
    // diagram. A `VecDeque` avoids the O(n) memmove that `Vec::drain(0..k)` cost when
    // the window overflows every step.
    history_1d: VecDeque<Vec<CellType>>, // past lines from oldest->newest (excluding current)
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
    /// Last `font_scale` actually pushed into the egui style, so the (fairly
    /// expensive) restyle only happens on the frames where it changed.
    applied_font_scale: f32,
    base_text_styles: BTreeMap<egui::TextStyle, egui::FontId>,

    // Statistics history for per-type counts (sliding window), keyed by interned symbol.
    stats_history: BTreeMap<Spur, VecDeque<(u64, u64)>>,
    stats_show: BTreeMap<Spur, bool>,
    stats_window_len: usize,

    // Rule editor state
    rule_edit_1d: Option<Rule1DEdit>,
    rule_edit_2d: Option<Rule2DEdit>,
    rule_error_msg: Option<String>,
    // Extra declared types added via UI (beyond those seen in rules/initial grid)
    custom_types: BTreeSet<Spur>,

    // Transient input for adding a new type/state
    new_type_name: String,

    // UI: visibility of the right-side Rule Editor panel
    show_rule_editor: bool,

    // Grid size configuration (editable by user)
    grid_width: usize,
    grid_height: usize,

    // Simulation timer (tracks wall-clock time while playing)
    /// Accumulated wall-clock duration across all play segments.
    sim_elapsed: Duration,
    /// Number of steps taken while the timer was active (for per-step average).
    sim_timed_steps: u64,
    /// Instant when the current play segment started (None when paused).
    sim_play_start: Option<Instant>,
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
            shape_buf: Vec::new(),
            inactive_color: Color32::from_rgb(30, 30, 35),
            show_grid_lines: true,
            grid_line_color: Color32::from_rgb(60, 60, 70),
            history_1d: VecDeque::new(),
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
            applied_font_scale: 1.0,
            // Text styles are theme-independent, so either theme's style is a fine
            // baseline to scale from.
            base_text_styles: cc.egui_ctx.style_of(egui::Theme::Dark).text_styles.clone(),
            stats_history: BTreeMap::new(),
            stats_show: BTreeMap::new(),
            stats_window_len: 300,
            // Rule editor defaults
            rule_edit_1d: None,
            rule_edit_2d: None,
            rule_error_msg: None,
            custom_types: BTreeSet::new(),
            new_type_name: String::new(),
            show_rule_editor: true,
            grid_width: 50,
            grid_height: 30,
            sim_elapsed: Duration::ZERO,
            sim_timed_steps: 0,
            sim_play_start: None,
        };
        // Start with a default 2D Life-like demo
        app.load_demo_life();
        app
    }

    #[inline]
    fn inactive_color(&self) -> Color32 { self.inactive_color }

    fn set_color_for(&mut self, ty: &CellType, color: Color32) {
        if ty.as_str() == INACTIVE { self.inactive_color = color; return; }
        self.colors.insert(ty.0, color);
    }

    fn color_of(&self, ty: &CellType) -> Color32 {
        color_for(*ty, &self.colors, &self.palette, self.inactive_color())
    }

    /// Apply user font scaling to egui text styles.
    ///
    /// Restyling forces egui to re-layout every galley, so this is a no-op unless
    /// the scale actually changed since the last frame.
    fn apply_font_scale(&mut self, ctx: &Context) {
        if self.font_scale == self.applied_font_scale { return; }
        self.applied_font_scale = self.font_scale;
        let mut map = self.base_text_styles.clone();
        for font in map.values_mut() {
            font.size = (font.size * self.font_scale).max(6.0);
        }
        ctx.all_styles_mut(|style| style.text_styles = map.clone());
    }

    /// Advance the automaton one step and maintain the 1D history buffer.
    fn step_once(&mut self) {
        // Snapshot the timer before stepping so we can measure elapsed time
        if let Some(start) = self.sim_play_start {
            self.sim_elapsed += start.elapsed();
            self.sim_play_start = Some(Instant::now());
        }
        match self.dim {
            Some(Dim::D1) => if let Some(g) = &mut self.d1 {
                // Push the current row onto the viewport's space-time history before
                // stepping. Recycle the row buffer that falls out of the window instead
                // of allocating a fresh `Vec` every step.
                let mut row = if self.history_1d.len() >= self.history_limit_1d {
                    let mut recycled = self.history_1d.pop_front().unwrap_or_default();
                    recycled.clear();
                    recycled
                } else {
                    Vec::new()
                };
                row.reserve(g.width);
                row.extend((0..g.width).map(|x| g.cell_type(x)));
                self.history_1d.push_back(row);
                while self.history_1d.len() > self.history_limit_1d { self.history_1d.pop_front(); }
                g.step();
            },
            Some(Dim::D2) => if let Some(g) = &mut self.d2 { g.step(); },
            None => {}
        }
        // Count this step for timing average (only when timer is running)
        if self.sim_play_start.is_some() {
            self.sim_timed_steps += 1;
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

    /// Collect all declared types for the current scenario (from rules/config),
    /// including Inactive, regardless of whether they are currently present on the grid.
    ///
    /// Ordered with `Inactive` first, then alphabetically.
    fn declared_types(&self) -> Vec<CellType> {
        let mut set: BTreeSet<Spur> = BTreeSet::new();
        set.insert(CellType::inactive().0);
        match self.dim {
            Some(Dim::D1) => if let Some(g) = &self.d1 {
                set.extend(g.rule.subrules.iter().flat_map(|s| {
                    [s.current_type.0, s.criteria_type.0, s.output_type.0]
                }));
            },
            Some(Dim::D2) => if let Some(g) = &self.d2 {
                set.extend(g.rule.subrules.iter().flat_map(|s| {
                    [s.current_type.0, s.criteria_type.0, s.output_type.0]
                }));
            },
            None => {}
        }
        // Include any extra types added via the editor
        set.extend(self.custom_types.iter().copied());

        let mut types: Vec<CellType> = set.into_iter().map(CellType).collect();
        sort_types_inactive_first(&mut types);
        types
    }

    /// Paint the grid directly using egui's Painter API with viewport culling.
    ///
    /// Only cells visible in the current scroll viewport are drawn, which
    /// eliminates the GPU texture-size limit that the old single-texture
    /// approach hit on large grids and dramatically improves performance
    /// because off-screen cells are skipped entirely.
    ///
    /// Within a visible row, horizontally adjacent cells of the same color are
    /// merged into a single rectangle. Cellular automata are highly spatially
    /// correlated, so this typically collapses hundreds of quads per row down to
    /// a handful and is the difference between the tessellator being the
    /// bottleneck and it being free.
    fn paint_grid_viewport(&mut self, ui: &mut egui::Ui) -> Option<egui::Response> {
        // Determine logical grid dimensions in cells
        let (grid_w, grid_h) = match self.dim {
            Some(Dim::D1) => {
                let g = self.d1.as_ref()?;
                let total_rows = self.history_1d.len() + 1;
                let visible_rows = total_rows.max(self.min_view_rows_1d.max(1));
                (g.width.max(1), visible_rows)
            }
            Some(Dim::D2) => {
                let g = self.d2.as_ref()?;
                (g.width.max(1), g.height.max(1))
            }
            None => return None,
        };

        let scale = self.scale.max(1) as f32;
        let total_size = egui::vec2(grid_w as f32 * scale, grid_h as f32 * scale);

        // Allocate space for the full grid so the scroll area knows the content size
        let (response, painter) = ui.allocate_painter(total_size, egui::Sense::click_and_drag());
        let full_rect = response.rect;

        // Determine visible region (clip rect intersected with allocated rect)
        let clip = ui.clip_rect();
        let visible = full_rect.intersect(clip);
        if visible.width() <= 0.0 || visible.height() <= 0.0 {
            return Some(response);
        }

        // Convert visible pixel range to cell range (with one cell margin for partial visibility)
        let cell_x_start = ((visible.min.x - full_rect.min.x) / scale).floor().max(0.0) as usize;
        let cell_y_start = ((visible.min.y - full_rect.min.y) / scale).floor().max(0.0) as usize;
        let cell_x_end = ((visible.max.x - full_rect.min.x) / scale).ceil().min(grid_w as f32) as usize;
        let cell_y_end = ((visible.max.y - full_rect.min.y) / scale).ceil().min(grid_h as f32) as usize;

        // Reuse last frame's shape allocation. Taking it out of `self` lets the rest
        // of this function borrow `self` immutably.
        let mut shapes = std::mem::take(&mut self.shape_buf);
        shapes.clear();
        let this = &*self;
        let bg = this.inactive_color();

        // Fill visible area with inactive background
        shapes.push(Shape::rect_filled(visible, 0.0, bg));

        let mut rows = RowPainter {
            shapes: &mut shapes,
            color_cache: Vec::new(),
            resolve: |ty| this.color_of(&ty),
            origin: full_rect.min,
            scale,
            bg,
        };

        // Draw only visible cells
        match this.dim {
            Some(Dim::D1) => {
                if let Some(g) = &this.d1 {
                    let history_len = this.history_1d.len();
                    // History rows
                    for row_i in cell_y_start..cell_y_end.min(history_len) {
                        let row = &this.history_1d[row_i];
                        let x_end = cell_x_end.min(row.len().min(g.width));
                        rows.emit_row(row_i, cell_x_start..x_end, |x| row[x]);
                    }
                    // Current row at y = history_len
                    if cell_y_end > history_len && cell_y_start <= history_len {
                        let x_end = cell_x_end.min(g.width);
                        rows.emit_row(history_len, cell_x_start..x_end, |x| g.cell_type(x));
                    }
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &this.d2 {
                    let w = g.width;
                    let x_end = cell_x_end.min(w);
                    for y in cell_y_start..cell_y_end.min(g.height) {
                        rows.emit_row(y, cell_x_start..x_end, |x| g.cell_type(y * w + x));
                    }
                }
            }
            None => {}
        }

        // Grid lines (only for visible cells; skip when scale < 3 as lines would dominate)
        if this.show_grid_lines && scale >= 3.0 {
            let stroke = egui::Stroke::new(1.0, this.grid_line_color);
            // Vertical lines
            for cx in cell_x_start..=cell_x_end.min(grid_w) {
                let px = full_rect.min.x + cx as f32 * scale;
                shapes.push(Shape::line_segment(
                    [egui::pos2(px, visible.min.y), egui::pos2(px, visible.max.y)],
                    stroke,
                ));
            }
            // Horizontal lines
            for cy in cell_y_start..=cell_y_end.min(grid_h) {
                let py = full_rect.min.y + cy as f32 * scale;
                shapes.push(Shape::line_segment(
                    [egui::pos2(visible.min.x, py), egui::pos2(visible.max.x, py)],
                    stroke,
                ));
            }
        }

        // One batched hand-off to the painter instead of a lock per shape.
        painter.extend(shapes.drain(..));
        self.shape_buf = shapes;

        Some(response)
    }

    /// Build the top toolbar: play/pause, step, run-to, scale, export/save/reset.
    fn ui_top_controls(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        ui.horizontal(|ui| {
            if ui.button(if self.playing { "Pause" } else { "Play" }).clicked() {
                self.playing = !self.playing;
                self.last_tick = Instant::now();
                if self.playing {
                    // Start a new play segment for the timer
                    self.sim_play_start = Some(Instant::now());
                    self.set_status("Playing");
                } else {
                    // Pause: flush the current play segment into accumulated elapsed
                    if let Some(start) = self.sim_play_start.take() {
                        self.sim_elapsed += start.elapsed();
                    }
                    self.set_status("Paused");
                }
            }
            if ui.button("Step").clicked() { self.step_once(); self.set_status(format!("Stepped to {}", self.current_step())); }
            ui.add(egui::DragValue::new(&mut self.refresh_ms).range(10..=2000).suffix(" ms"));
            ui.label("Refresh");
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.run_to_steps).range(1..=1_000_000).suffix(" steps"));
            if ui.button("Run to +N").clicked() {
                let target = self.current_step().saturating_add(self.run_to_steps);
                self.run_to_target = Some(target);
                self.playing = true; // ensure stepping
                if self.sim_play_start.is_none() {
                    self.sim_play_start = Some(Instant::now());
                }
                self.set_status(format!("Running to {}", target));
            }
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.scale).range(1..=64).suffix(" px"));
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
            ui.vertical(|ui| {
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
                // text edit with a smaller area
                ui.add_sized([20.0, 20.0], TextEdit::singleline(&mut self.custom_code_input));
                ui.label("n:");
                ui.add(egui::DragValue::new(&mut self.custom_n).range(1..=8));
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
                ui.add(egui::DragValue::new(&mut self.history_limit_1d).range(1..=10_000));
            });
            ui.separator();
            ui.label("Grid size:");
            ui.horizontal(|ui| {
                ui.label("W:");
                ui.add(egui::DragValue::new(&mut self.grid_width).range(1..=2000).speed(1));
                if matches!(self.dim, Some(Dim::D2)) {
                    ui.label("H:");
                    ui.add(egui::DragValue::new(&mut self.grid_height).range(1..=2000).speed(1));
                }
                if ui.button("Resize").on_hover_text("Rebuild the grid with the specified dimensions. Existing cells are preserved where they overlap; new cells are Inactive.").clicked() {
                    self.resize_grid();
                }
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
                        self.custom_types.insert(interner().get_or_intern(name));
                        self.set_status(format!("Added type '{}'", name));
                        // set a default color if desired (optional; fallback hash works)
                        self.new_type_name.clear();
                    }
                }
            });
            // Show current list (already ordered Inactive-first by `declared_types`)
            let names: Vec<&'static str> = self.declared_types().iter().map(|t| t.as_str()).collect();
            ui.horizontal_wrapped(|ui| {
                for n in &names { ui.label(egui::RichText::new(*n).monospace()); }
            });
            ui.separator();

            if let Some(dim) = self.dim {
                match dim {
                    Dim::D1 => {
                        // Ensure editor model exists
                        if self.rule_edit_1d.is_none()
                            && let Some(g) = &self.d1 { self.rule_edit_1d = Some(Rule1DEdit::from_rule(&g.rule)); }
                        if let Some(edit) = &mut self.rule_edit_1d {
                            // Subrules list (scrollable)
                            let mut remove_idx: Option<usize> = None;
                            let mut move_up_idx: Option<usize> = None;
                            let mut move_down_idx: Option<usize> = None;
                            ui.set_min_height(240.0);
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                for i in 0..edit.subrules.len() {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("Subrule #{}", i+1));
                                        if ui.button("Remove").clicked() { remove_idx = Some(i); }
                                    });
                                    let ty_names = &names;
                                    let sub = &mut edit.subrules[i];
                                    // current
                                    ui.horizontal(|ui| {
                                        ui.label("current:")
                                            .on_hover_text("Center cell must currently be this state for the subrule to apply.");
                                        let mut sel = sub.current.clone();
                                        egui::ComboBox::from_id_salt(format!("d1_cur_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| {
                                                for n in ty_names { ui.selectable_value(&mut sel, (*n).to_owned(), *n); }
                                            });
                                        if sel != sub.current { sub.current = sel; }
                                    });
                                    // criteria
                                    ui.horizontal(|ui| {
                                        ui.label("criteria:")
                                            .on_hover_text("Neighbor cells equal to this state are treated as 1s in the Wolfram pattern; others are 0s.");
                                        let mut sel = sub.criteria.clone();
                                        egui::ComboBox::from_id_salt(format!("d1_crit_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| {
                                                for n in ty_names { ui.selectable_value(&mut sel, (*n).to_owned(), *n); }
                                            });
                                        if sel != sub.criteria { sub.criteria = sel; }
                                    });
                                    // output
                                    ui.horizontal(|ui| {
                                        ui.label("output:")
                                            .on_hover_text("The new state to set when this subrule matches.");
                                        let mut sel = sub.output.clone();
                                        egui::ComboBox::from_id_salt(format!("d1_out_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| {
                                                for n in ty_names { ui.selectable_value(&mut sel, (*n).to_owned(), *n); }
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
                                        ui.add(egui::DragValue::new(&mut sub.n).range(1..=8))
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
                            if let Some(i) = move_up_idx && i > 0 { edit.subrules.swap(i, i - 1); }
                            if let Some(i) = move_down_idx && i + 1 < edit.subrules.len() { edit.subrules.swap(i, i + 1); }
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
                        if self.rule_edit_2d.is_none()
                            && let Some(g) = &self.d2 { self.rule_edit_2d = Some(Rule2DEdit::from_rule(&g.rule)); }
                        if let Some(edit) = &mut self.rule_edit_2d {
                            let mut remove_idx: Option<usize> = None;
                            let mut move_up_idx: Option<usize> = None;
                            let mut move_down_idx: Option<usize> = None;
                            ui.set_min_height(240.0);
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                for i in 0..edit.subrules.len() {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("Subrule #{}", i+1));
                                        if ui.button("Remove").clicked() { remove_idx = Some(i); }
                                    });
                                    let ty_names = &names;
                                    let sub = &mut edit.subrules[i];
                                    // current
                                    ui.horizontal(|ui| {
                                        ui.label("current:")
                                            .on_hover_text("Center cell must currently be this state for the subrule to apply.");
                                        let mut sel = sub.current.clone();
                                        egui::ComboBox::from_id_salt(format!("d2_cur_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| { for n in ty_names { ui.selectable_value(&mut sel, (*n).to_owned(), *n); } });
                                        if sel != sub.current { sub.current = sel; }
                                    });
                                    // criteria
                                    ui.horizontal(|ui| {
                                        ui.label("criteria:")
                                            .on_hover_text("Neighbor cells of this state are counted within the chosen neighborhood.");
                                        let mut sel = sub.criteria.clone();
                                        egui::ComboBox::from_id_salt(format!("d2_crit_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| { for n in ty_names { ui.selectable_value(&mut sel, (*n).to_owned(), *n); } });
                                        if sel != sub.criteria { sub.criteria = sel; }
                                    });
                                    // output
                                    ui.horizontal(|ui| {
                                        ui.label("output:")
                                            .on_hover_text("The new state to set when this subrule matches.");
                                        let mut sel = sub.output.clone();
                                        egui::ComboBox::from_id_salt(format!("d2_out_{}", i))
                                            .selected_text(sel.clone())
                                            .show_ui(ui, |ui| { for n in ty_names { ui.selectable_value(&mut sel, (*n).to_owned(), *n); } });
                                        if sel != sub.output { sub.output = sel; }
                                    });
                                    // neighborhood modifiers
                                    ui.horizontal(|ui| {
                                        ui.label("count:")
                                            .on_hover_text("Baseline neighbor count for comparison. See 'op' for how it is used.");
                                        ui.add(egui::DragValue::new(&mut sub.count).range(0..=99))
                                            .on_hover_text("Set the baseline count between 0 and 99.");
                                        ui.label("op:")
                                            .on_hover_text("Comparison: gt means >= count, lt means <= count, eq means exactly count. With a limit, you can specify a range.");
                                        let mut op = sub.op; 
                                        egui::ComboBox::from_id_salt(format!("d2_op_{}", i))
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
                                            ui.add(egui::DragValue::new(&mut sub.limit_value).range(0..=99))
                                                .on_hover_text("Inclusive bound for the range comparison."); 
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("range n:")
                                            .on_hover_text("Neighborhood range (>=1). The square window is (2n+1)^2, filtered by the chosen neighborhood type.");
                                        ui.add(egui::DragValue::new(&mut sub.range).range(1..=8))
                                            .on_hover_text("Set range n between 1 and 8.");
                                        ui.label("neighborhood:")
                                            .on_hover_ui(|ui| {
                                                ui.label("Neighborhood shape around the center (@):");
                                                let diag = CellaApp::neighborhood_ascii(sub.range, sub.neighborhood);
                                                ui.monospace(diag);
                                                ui.small("Legend: @ center, # counted neighbor, . outside");
                                                ui.separator();
                                                ui.label("Moore = square; VonNeumann = Manhattan distance; Langton = diagonals; StraightLine = cardinal lines only; Knight = chess knight L-moves (range = max hops)");
                                            });
                                        let mut nb = sub.neighborhood;
                                        egui::ComboBox::from_id_salt(format!("d2_nh_{}", i))
                                            .selected_text(match nb { Neighborhood2D::Moore=>"Moore", Neighborhood2D::VonNeumann=>"VonNeumann", Neighborhood2D::Langton =>"Langton", Neighborhood2D::StraightLine=>"StraightLine", Neighborhood2D::Knight=>"Knight" })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut nb, Neighborhood2D::Moore, "Moore");
                                                ui.selectable_value(&mut nb, Neighborhood2D::VonNeumann, "VonNeumann");
                                                ui.selectable_value(&mut nb, Neighborhood2D::Langton, "Langton");
                                                ui.selectable_value(&mut nb, Neighborhood2D::StraightLine, "StraightLine");
                                                ui.selectable_value(&mut nb, Neighborhood2D::Knight, "Knight");
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
                            if let Some(i) = move_up_idx && i > 0 { edit.subrules.swap(i, i - 1); }
                            if let Some(i) = move_down_idx && i + 1 < edit.subrules.len() { edit.subrules.swap(i, i + 1); }
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
                if ty == CellType::inactive() { continue; }
                let mut col = self.color_of(&ty);
                if ui.color_edit_button_srgba(&mut col).changed() {
                    self.set_color_for(&ty, col);
                }
                ui.label(ty.as_str());
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
            // Cap steps per frame so a large "Run to +N" still lets the UI render.
            for _ in 0..RUN_TO_STEPS_PER_FRAME {
                if self.current_step() >= target { break; }
                self.step_once();
            }
            if self.current_step() >= target {
                self.run_to_target = None;
                self.playing = false;
                // Stop timer when run-to completes
                if let Some(start) = self.sim_play_start.take() {
                    self.sim_elapsed += start.elapsed();
                }
            }
        }
    }

    /// Resize the current grid to `grid_width` x `grid_height`, preserving existing
    /// cell data where it overlaps and filling new cells with Inactive.
    fn resize_grid(&mut self) {
        let new_w = self.grid_width.max(1);
        let new_h = self.grid_height.max(1);
        match self.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.d1 {
                    let old_w = g.width;
                    let rule = g.rule.clone();
                    let hist = g.history_limit;
                    let mut init: Vec<CellType> = vec![CellType::inactive(); new_w];
                    for (x, cell) in init.iter_mut().take(new_w.min(old_w)).enumerate() {
                        *cell = g.cell_type(x);
                    }
                    self.d1 = Some(Grid1D::new(new_w, hist, init, rule));
                    self.initial_state = self.d1.as_ref().map(GridState::from_grid1d);
                    self.history_1d.clear();
                    self.undo_stack.clear();
                    self.current_paint_batch = None;
                    self.stats_clear_and_init();
                    self.set_status(format!("Resized 1D grid to width {}", new_w));
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.d2 {
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
                    self.d2 = Some(Grid2D::new(new_w, new_h, hist, init, rule));
                    self.initial_state = self.d2.as_ref().map(GridState::from_grid2d);
                    self.history_1d.clear();
                    self.undo_stack.clear();
                    self.current_paint_batch = None;
                    self.stats_clear_and_init();
                    self.set_status(format!("Resized 2D grid to {}×{}", new_w, new_h));
                }
            }
            None => {}
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
        self.grid_width = w; self.grid_height = h;
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
        self.grid_width = width; self.grid_height = 1;
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
        self.grid_width = width; self.grid_height = 1;
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
        self.grid_width = w; self.grid_height = h;
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
        self.grid_width = w; self.grid_height = h;
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
            self.grid_width = width; self.grid_height = 1;
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
                            if let Some(gr) = &self.d1 {
                                self.initial_state = Some(GridState::from_grid1d(gr));
                                self.grid_width = gr.width; self.grid_height = 1;
                            }
                            self.history_1d.clear(); self.undo_stack.clear(); self.current_paint_batch = None; self.colors.clear();
                            self.update_selected_draw_type_default();
                            self.stats_clear_and_init();
                        }
                    }
                    config::CellaConfig::D2(_) => {
                        if let Some(g) = cfg.build_grid2d() {
                            self.dim = Some(Dim::D2); self.d2 = Some(g); self.d1 = None;
                            self.set_status(format!("Loaded config (2D): {}", name));
                            if let Some(gr) = &self.d2 {
                                self.initial_state = Some(GridState::from_grid2d(gr));
                                self.grid_width = gr.width; self.grid_height = gr.height;
                            }
                            self.history_1d.clear(); self.undo_stack.clear(); self.current_paint_batch = None; self.colors.clear();
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

    // ----- Save/Export -----
    fn save_final_state(&mut self) {
        let state = match self.dim {
            Some(Dim::D1) => self.d1.as_ref().map(GridState::from_grid1d),
            Some(Dim::D2) => self.d2.as_ref().map(GridState::from_grid2d),
            None => None,
        };
        if let Some(st) = state
            && let Some(path) = FileDialog::new().set_file_name("snapshot.json").save_file() {
                let json = serde_json::to_string_pretty(&st).unwrap();
                let _ = std::fs::write(path, json);
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
            // `GifExport` borrows the color tables, so it is built inside the worker
            // thread that owns the clones.
            match self.dim {
                Some(Dim::D1) => if let Some(g) = &self.d1 {
                    let mut grid_clone = g.clone();
                    let history_opt = if self.export_1d_with_history { Some(self.history_limit_1d) } else { None };
                    let handle = std::thread::spawn(move || {
                        let opts = GifExport {
                            path, steps, fps, scale,
                            colors: &colors, palette: &palette, inactive,
                            progress: Some(&progress),
                        };
                        export_gif_1d(&mut grid_clone, &opts, history_opt).map_err(|e| e.to_string())
                    });
                    self.export_join = Some(handle);
                },
                Some(Dim::D2) => if let Some(g) = &self.d2 {
                    let mut grid_clone = g.clone();
                    let handle = std::thread::spawn(move || {
                        let opts = GifExport {
                            path, steps, fps, scale,
                            colors: &colors, palette: &palette, inactive,
                            progress: Some(&progress),
                        };
                        export_gif_2d(&mut grid_clone, &opts).map_err(|e| e.to_string())
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
        // Reset simulation timer
        self.sim_elapsed = Duration::ZERO;
        self.sim_timed_steps = 0;
        self.sim_play_start = None;
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
                pick = (0..g.width).map(|i| g.cell_type(i)).find(|t| *t != CellType::inactive());
            },
            Some(Dim::D2) => if let Some(g) = &self.d2 {
                pick = (0..g.width * g.height).map(|i| g.cell_type(i)).find(|t| *t != CellType::inactive());
            },
            None => {}
        }
        self.selected_draw_type = Some(pick.unwrap_or_else(CellType::inactive));
    }

    /// Internal: clear and initialize statistics history/toggles from current grid.
    /// Also resets the simulation timer.
    fn stats_clear_and_init(&mut self) {
        self.sim_elapsed = Duration::ZERO;
        self.sim_timed_steps = 0;
        self.sim_play_start = None;
        self.stats_history.clear();
        self.stats_show.clear();
        let inactive = CellType::inactive().0;
        let (mut entries, step): (Vec<(Spur, u64)>, u64) = match self.dim {
            Some(Dim::D1) => match &self.d1 {
                Some(g) => (g.counts_current.iter().map(|(k, v)| (*k, *v)).collect(), g.step),
                None => return,
            },
            Some(Dim::D2) => match &self.d2 {
                Some(g) => (g.counts_current.iter().map(|(k, v)| (*k, *v)).collect(), g.step),
                None => return,
            },
            None => return,
        };
        if !entries.iter().any(|(k, _)| *k == inactive) { entries.push((inactive, 0)); }
        // Order: Inactive first, then by name
        entries.sort_by(|a, b| {
            let (a, b) = (interner().resolve(&a.0), interner().resolve(&b.0));
            (a != INACTIVE).cmp(&(b != INACTIVE)).then_with(|| a.cmp(b))
        });
        // Default visibility: first 9 active types (Inactive off by default)
        let mut shown_left = 9usize;
        for (k, c) in entries {
            let show = if k == inactive { false } else if shown_left > 0 { shown_left -= 1; true } else { false };
            self.stats_show.insert(k, show);
            self.stats_history.insert(k, VecDeque::from(vec![(step, c)]));
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
    ///
    /// The series maps are moved out of `self` for the duration so the grid's
    /// `counts_current` can be read in place rather than cloned every single step.
    fn stats_record_step(&mut self) {
        let mut history = std::mem::take(&mut self.stats_history);
        let mut show = std::mem::take(&mut self.stats_show);
        let window = self.stats_window_len.max(1);
        let inactive = CellType::inactive().0;

        let current = match self.dim {
            Some(Dim::D1) => self.d1.as_ref().map(|g| (&g.counts_current, g.step)),
            Some(Dim::D2) => self.d2.as_ref().map(|g| (&g.counts_current, g.step)),
            None => None,
        };
        if let Some((counts, step)) = current {
            // Ensure entries for any newly seen types (default hidden, including Inactive)
            for k in counts.keys().copied().chain(std::iter::once(inactive)) {
                history.entry(k).or_default();
                show.entry(k).or_insert(false);
            }
            // Append a sample to every tracked series; types absent this step record 0.
            for (k, list) in history.iter_mut() {
                list.push_back((step, counts.get(k).copied().unwrap_or(0)));
                while list.len() > window { list.pop_front(); }
            }
        }

        self.stats_history = history;
        self.stats_show = show;
    }

    /// Show a collapsible panel with per-type statistics (current and peak counts),
    /// and a running history chart (fixed-size window) similar to Task Manager.
    fn ui_statistics(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Statistics", |ui| {
            // Current/peak table
            let mut entries: Vec<(CellType, u64, u64)> = Vec::new();
            {
                let counts_and_peaks = match self.dim {
                    Some(Dim::D1) => self.d1.as_ref().map(|g| (&g.counts_current, &g.peak_counts)),
                    Some(Dim::D2) => self.d2.as_ref().map(|g| (&g.counts_current, &g.peak_counts)),
                    None => None,
                };
                if let Some((counts, peaks)) = counts_and_peaks {
                    let keys: BTreeSet<Spur> = counts.keys().chain(peaks.keys()).copied().collect();
                    entries.extend(keys.into_iter().map(|k| {
                        (CellType(k), counts.get(&k).copied().unwrap_or(0), peaks.get(&k).copied().unwrap_or(0))
                    }));
                }
            }
            entries.sort_by(|a, b| {
                let (a, b) = (a.0.as_str(), b.0.as_str());
                (a != INACTIVE).cmp(&(b != INACTIVE)).then_with(|| a.cmp(b))
            });
            let total = entries.iter().fold(0u64, |acc, (_, c, _)| acc.saturating_add(*c));
            ui.label(format!("Total cells: {}", total));
            for (ty, cur, peak) in &entries {
                ui.label(format!("{:>10}: {} (peak {})", ty.as_str(), cur, peak));
            }
            ui.separator();

            // Visibility toggles
            ui.label("Series shown in graph:");
            let mut series: Vec<CellType> = self.stats_history.keys().copied().map(CellType).collect();
            sort_types_inactive_first(&mut series);
            ui.horizontal_wrapped(|ui| {
                for ty in &series {
                    let mut show = self.stats_show.get(&ty.0).copied().unwrap_or(false);
                    let label = egui::RichText::new(ty.as_str()).color(self.color_of(ty));
                    if ui.checkbox(&mut show, label).changed() {
                        self.stats_show.insert(ty.0, show);
                    }
                }
            });

            // Line plot of the last N samples per selected series
            let plot = Plot::new("stats_plot").legend(Legend::default());
            plot.show(ui, |plot_ui| {
                for ty in &series {
                    if !self.stats_show.get(&ty.0).copied().unwrap_or(false) { continue; }
                    let Some(list) = self.stats_history.get(&ty.0) else { continue };
                    if list.is_empty() { continue; }
                    let pts: PlotPoints = list.iter().map(|(s, v)| [*s as f64, *v as f64]).collect::<Vec<_>>().into();
                    plot_ui.line(Line::new(ty.as_str(), pts).color(self.color_of(ty)));
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
            Neighborhood2D::Langton => "Langton",
            Neighborhood2D::StraightLine => "StraightLine",
            Neighborhood2D::Knight => "Knight",
        };
        out.push_str(&format!("{} (n={})\n", name, range));
        // Knight moves can reach up to 2*range steps per axis, so widen the window.
        let half = if kind == Neighborhood2D::Knight { n * 2 } else { n };
        for dy in -half..=half {
            for dx in -half..=half {
                if dx == 0 && dy == 0 {
                    out.push('@');
                } else {
                    let inside = neighborhood_contains(dx, dy, n, kind);
                    out.push(if inside { '#' } else { '.' });
                }
            }
            if dy != half { out.push('\n'); }
        }
        out
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
        egui::Panel::left("left_controls").default_size(260.0).show(ui, |ui| {
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
                        let mut names: Vec<String> = self.declared_types().into_iter().map(|t| t.as_str().to_string()).collect();
                        // Ensure ordering with Inactive first
                        names.sort();
                        names.sort_by_key(|a| a != INACTIVE);
                        let current_name = self.selected_draw_type.as_ref().map(|t| t.as_str()).unwrap_or_else(|| INACTIVE);
                        let mut sel = current_name;
                        egui::ComboBox::from_label("")
                            .selected_text(sel)
                            .show_ui(ui, |ui| {
                                for n in &names { ui.selectable_value(&mut sel, n.as_str(), n); }
                            });
                        if sel != current_name { self.selected_draw_type = Some(CellType::from(sel)); }
                    });
                    ui.label("Hold and drag on the grid while paused to paint.");
                });
                ui.separator();
                ui.collapsing("Export", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.export_steps).range(1..=10_000));
                        ui.label("steps");
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.export_fps).range(1..=60));
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

        // Right-side Rule Editor panel (resizable, can be hidden via toggle).
        // `show_collapsible` animates the slide in/out and lets a drag past the
        // minimum width collapse the panel, keeping `show_rule_editor` in sync with
        // the toolbar toggle.
        // Held in a local because `show_collapsible` writes back through the `&mut bool`
        // (drag-to-close), which would otherwise alias the `&mut self` the body needs.
        let mut show_editor = self.show_rule_editor;
        egui::Panel::right("right_rule_editor")
            .resizable(true)
            .min_size(220.0)
            .default_size(340.0)
            .show_collapsible(ui, &mut show_editor, |ui| {
                ui.heading("Rule Editor");
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.ui_rule_editor(ui);
                });
            });
        self.show_rule_editor = show_editor;

        egui::Panel::bottom("bottom_status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Step: {}", self.current_step()));
                // Show simulation timer when steps have been timed
                if self.sim_timed_steps > 0 || self.sim_play_start.is_some() {
                    let total = if let Some(start) = self.sim_play_start {
                        self.sim_elapsed + start.elapsed()
                    } else {
                        self.sim_elapsed
                    };
                    let total_secs = total.as_secs_f64();
                    let steps = self.sim_timed_steps.max(1);
                    let avg_ms = (total_secs * 1000.0) / steps as f64;
                    ui.separator();
                    ui.label(format!("Time: {:.2}s", total_secs));
                    ui.label(format!("Avg: {:.2} ms/step", avg_ms));
                }
                if let Some(msg) = &self.status_message {
                    ui.separator();
                    ui.label(egui::RichText::new(msg.clone()).italics());
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            // Hotkeys: Ctrl+Z undo last edit when paused
            if !self.playing {
                ui.input(|i| {
                    if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(Key::Z)
                        && let Some(batch) = self.undo_stack.pop() {
                            let undo_error = match self.dim {
                                Some(Dim::D1) => {
                                    let mut err = None;
                                    if let Some(g) = &mut self.d1 {
                                        for (idx, prev) in batch {
                                            if idx < g.width
                                                && let Some(e) = g.transition_state_and_buffer(idx, &prev){
                                                    err = Some(e);
                                                    break;
                                                }
                                        }
                                    }
                                    err
                                }
                                Some(Dim::D2) => {
                                    let mut err = None;
                                    if let Some(g) = &mut self.d2 {
                                        for (idx, prev) in batch {
                                            if idx < g.width * g.height
                                               && let Some(e) = g.transition_state_and_buffer(idx, &prev){
                                                    err = Some(e);
                                                    break;
                                               }
                                        }
                                    }
                                    err
                                }
                                None => None,
                            };
                            if let Some(err) = undo_error {
                                eprintln!("Undo error: {}", err);
                                self.set_status(format!("Undo error: {}", err));
                            }
                        }
                });
            }
            egui::ScrollArea::both()
                // Left-drag is reserved for painting, so never drag-to-scroll.
                .scroll_source(ScrollSource {
                    drag: DragScroll::Never,
                    scroll_bar: true,
                    mouse_wheel: true,
                })
                .show(ui, |ui| {
                if let Some(response) = self.paint_grid_viewport(ui) {

                    // Zoom with MouseWheel when hovered
                    if response.hovered() {
                        let dy = ui.input(|i| i.smooth_scroll_delta.y);
                        if dy > 0.0 { self.scale = (self.scale + 1).min(64); }
                        else if dy < 0.0 { self.scale = self.scale.saturating_sub(1).max(1); }
                    }

                    if response.dragged() {
                        // Right mouse drag to pan the scroll area (so left is free for painting)
                        let (right_down, delta) = ui.input(|i| (i.pointer.secondary_down(), i.pointer.delta()));
                        if right_down
                            && (delta.x != 0.0 || delta.y != 0.0) { ui.scroll_with_delta(delta); }
                    }

                    // Painting mode: click/drag to set cells when paused
                    if !self.playing && matches!(self.draw_mode, DrawMode::Paint) {
                        let is_down = ui.input(|i| i.pointer.primary_down());
                        if is_down {
                            if let Some(pos) = ui.input(|i| i.pointer.hover_pos())
                                && response.rect.contains(pos) {
                                    let local = pos - response.rect.min;
                                    let px = local.x.max(0.0) as usize;
                                    let py = local.y.max(0.0) as usize;
                                    let cell_x = px / self.scale.max(1);
                                    let cell_y = py / self.scale.max(1);
                                    let paint_ty = self.selected_draw_type.unwrap_or_else(CellType::inactive);
                                    match self.dim {
                                        Some(Dim::D1) => {
                                            if let Some(g) = &mut self.d1 {
                                                let total_rows = self.history_1d.len() + 1;
                                                if total_rows > 0 && cell_y == total_rows - 1 && cell_x < g.width {
                                                    let idx = cell_x;
                                                    let prev = g.cell_type(idx);
                                                    if prev != paint_ty {
                                                        if self.current_paint_batch.is_none() { self.current_paint_batch = Some(Vec::new()); }
                                                        if let Some(batch) = &mut self.current_paint_batch
                                                            && !batch.iter().any(|(j, _)| *j == idx) { batch.push((idx, prev)); }
                                                        if let Some(err) = g.transition_state_and_buffer(idx, &paint_ty) {
                                                            eprintln!("Paint error: {}", err);
                                                            self.set_status(format!("Paint error: {}", err));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Some(Dim::D2) => {
                                            if let Some(g) = &mut self.d2
                                                && cell_x < g.width && cell_y < g.height {
                                                    let idx = cell_y * g.width + cell_x;
                                                    let prev = g.cell_type(idx);
                                                    if prev != paint_ty {
                                                        if self.current_paint_batch.is_none() { self.current_paint_batch = Some(Vec::new()); }
                                                        if let Some(batch) = &mut self.current_paint_batch
                                                            && !batch.iter().any(|(j, _)| *j == idx) { batch.push((idx, prev)); }
                                                        if let Some(err) = g.transition_state_and_buffer(idx, &paint_ty) {
                                                            eprintln!("Paint error: {}", err);
                                                            self.set_status(format!("Paint error: {}", err));
                                                        }
                                                    }
                                                }
                                        }
                                        None => {}
                                    }
                                }
                        } else {
                            if let Some(batch) = self.current_paint_batch.take() && !batch.is_empty() { self.undo_stack.push(batch); }
                        }
                    }

                    // Cycle mode: Click to edit when paused
                    if response.clicked() && !self.playing && matches!(self.draw_mode, DrawMode::Cycle)
                        && let Some(pos) = response.interact_pointer_pos() {
                            let local = pos - response.rect.min;
                            let px = local.x.max(0.0) as usize;
                            let py = local.y.max(0.0) as usize;
                            let cell_x = px / self.scale.max(1);
                            let cell_y = py / self.scale.max(1);
                            // Cycle through every *declared* type, not just the ones that
                            // happen to be on the grid right now — otherwise a state that
                            // has died out becomes impossible to paint back in. Computed
                            // before the grid is mutably borrowed below.
                            let cycle_types = self.declared_types();
                            match self.dim {
                                Some(Dim::D1) => {
                                    if let Some(g) = &mut self.d1 {
                                        let total_rows = self.history_1d.len() + 1;
                                        if cell_y == total_rows - 1 && cell_x < g.width {
                                            let current = g.cell_type(cell_x);
                                            let next = next_in_cycle(&cycle_types, current);
                                            // push undo batch of one cell
                                            self.undo_stack.push(vec![(cell_x, current)]);
                                            if let Some(err) = g.transition_state_and_buffer(cell_x, &next) {
                                                eprintln!("Cycle edit error: {}", err);
                                                self.set_status(format!("Cycle edit error: {}", err));
                                            }
                                        }
                                    }
                                }
                                Some(Dim::D2) => {
                                    if let Some(g) = &mut self.d2
                                        && cell_x < g.width && cell_y < g.height {
                                            let i = cell_y * g.width + cell_x;
                                            let current = g.cell_type(i);
                                            let next = next_in_cycle(&cycle_types, current);
                                            self.undo_stack.push(vec![(i, current)]);
                                            if let Some(err) = g.transition_state_and_buffer(i, &next) {
                                                eprintln!("Cycle edit error: {}", err);
                                                self.set_status(format!("Cycle edit error: {}", err));
                                            }
                                        }
                                }
                                None => {}
                            }
                        }
                } else {
                    ui.label("No grid loaded.");
                }
            });
        });

        // If an export thread is active, poll for completion and finalize
        if let Some(handle) = &self.export_join
            && handle.is_finished() {
                if let Some(handle) = self.export_join.take() {
                    match handle.join().unwrap_or_else(|_| Err("export thread panicked".to_string())) {
                        Ok(()) => { self.export_message = Some("Export complete".into()); self.set_status("Export complete"); },
                        Err(e) => { let msg = format!("Export failed: {}", e); self.export_message = Some(msg.clone()); self.set_status(msg); },
                    }
                }
                self.export_progress = None;
                self.export_total = 0;
            }

        self.tick_play();

        // Only drive continuous repaints when something is actually animating.
        // Previously this pinned the app at ~100 fps (and full CPU/GPU) even while
        // paused with nothing on screen changing; egui repaints on input anyway.
        if self.playing || self.run_to_target.is_some() {
            // Wake up in time for the next simulation tick, capped so the timer
            // readout in the status bar still updates smoothly.
            ctx.request_repaint_after(Duration::from_millis(self.refresh_ms.min(100)));
        } else if self.export_join.is_some() {
            // Poll the export thread's progress a few times a second.
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::render::palette_index_for;

    const RED: Color32 = Color32::RED;
    const BLUE: Color32 = Color32::BLUE;
    const BG: Color32 = Color32::BLACK;

    /// Run `emit_row` over `cells` and return the (x, width) of each emitted rect
    /// in cell units, so expectations read in grid coordinates rather than pixels.
    fn runs_for(cells: &[&str], xs: std::ops::Range<usize>) -> Vec<(usize, usize)> {
        const SCALE: f32 = 4.0;
        let types: Vec<CellType> = cells.iter().map(|s| CellType::from(*s)).collect();
        let mut shapes = Vec::new();
        let mut painter = RowPainter {
            shapes: &mut shapes,
            color_cache: Vec::new(),
            resolve: |ty: CellType| match ty.as_str() {
                "R" => RED,
                "B" => BLUE,
                _ => BG,
            },
            origin: egui::pos2(0.0, 0.0),
            scale: SCALE,
            bg: BG,
        };
        painter.emit_row(0, xs, |x| types[x]);
        shapes
            .iter()
            .map(|s| match s {
                Shape::Rect(r) => (
                    (r.rect.min.x / SCALE).round() as usize,
                    (r.rect.width() / SCALE).round() as usize,
                ),
                other => panic!("expected a rect, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn adjacent_same_color_cells_merge_into_one_rect() {
        assert_eq!(runs_for(&["R", "R", "R", "R"], 0..4), vec![(0, 4)]);
    }

    #[test]
    fn background_runs_are_not_painted() {
        // Only the two "R" cells at x=1..3 should produce geometry.
        assert_eq!(runs_for(&["_", "R", "R", "_"], 0..4), vec![(1, 2)]);
    }

    #[test]
    fn distinct_colors_split_into_separate_rects() {
        assert_eq!(
            runs_for(&["R", "R", "B", "R"], 0..4),
            vec![(0, 2), (2, 1), (3, 1)]
        );
    }

    #[test]
    fn trailing_run_is_flushed() {
        // Regression guard: the final run must be emitted when the row ends
        // mid-run rather than on a color change.
        assert_eq!(runs_for(&["B", "R", "R"], 0..3), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn only_the_requested_x_window_is_painted() {
        assert_eq!(runs_for(&["R", "R", "R", "R"], 1..3), vec![(1, 2)]);
    }

    #[test]
    fn empty_range_emits_nothing() {
        assert_eq!(runs_for(&["R", "R"], 1..1), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn all_background_row_emits_nothing() {
        assert_eq!(runs_for(&["_", "_", "_"], 0..3), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn next_in_cycle_wraps_and_handles_unknown_types() {
        let types = [CellType::from("a"), CellType::from("b"), CellType::from("c")];
        assert_eq!(next_in_cycle(&types, types[0]), types[1]);
        assert_eq!(next_in_cycle(&types, types[2]), types[0]);
        // A type that is not declared restarts the cycle at the second entry.
        assert_eq!(next_in_cycle(&types, CellType::from("zzz")), types[1]);
        assert_eq!(next_in_cycle(&[], types[0]), CellType::inactive());
    }

    #[test]
    fn types_sort_with_inactive_first_then_alphabetical() {
        let mut types = vec![
            CellType::from("zeta"),
            CellType::from("alpha"),
            CellType::inactive(),
            CellType::from("beta"),
        ];
        sort_types_inactive_first(&mut types);
        let names: Vec<&str> = types.iter().map(|t| t.as_str()).collect();
        assert_eq!(names, vec![INACTIVE, "alpha", "beta", "zeta"]);
    }

    #[test]
    fn palette_index_is_deterministic_and_in_range() {
        assert_eq!(palette_index_for("Alive", 8), palette_index_for("Alive", 8));
        assert!(palette_index_for("Alive", 8) < 8);
        // Must not divide by zero on an empty palette.
        assert_eq!(palette_index_for("Alive", 0), 0);
    }
}
