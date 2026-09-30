//! The application's state, grouped by what it is *for*.
//!
//! `CellaApp` used to be one flat list of fifty fields, which made it
//! impossible to see at a glance which ones belonged together. Each struct
//! here owns one concern, so a function that only paces playback can borrow
//! [`Playback`] without also locking the export machinery.
//!
//! Every struct carries the application's starting values in its `Default`
//! impl, so `CellaApp::new` reads as a list of deliberate overrides rather
//! than fifty assignments.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use super::app::{Dim, DrawMode};
use super::layers::LayerState;
use super::panels::rule_edit_model::{Rule1DEdit, Rule2DEdit};
use super::render::default_palette;
use super::theme::ThemeChoice;
use cella_lib::*;
use egui::{Color32, Shape};
use lasso2::Spur;

/// What is being simulated: the grid itself, plus the snapshot "Reset" returns to.
#[derive(Default)]
pub(in crate::gui) struct Scenario {
    pub(in crate::gui) d1: Option<Grid1D>,
    pub(in crate::gui) d2: Option<Grid2D>,
    pub(in crate::gui) dim: Option<Dim>,
    pub(in crate::gui) initial_state: Option<GridState>,
}

/// How playback is paced: one step per interval, or as many as fit in a frame budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::gui) enum Pacing {
    /// One step per `refresh_ms`, the animation speed the user chose.
    Interval,
    /// As many steps as fit in one frame's time budget. Reserved for the
    /// "Max speed" playback setting; nothing selects it yet.
    Unbounded,
}

/// Play/pause, the "Run to +N" target, and the stopwatch behind the
/// "Avg ms/step" readout in the status bar.
pub(in crate::gui) struct Playback {
    pub(in crate::gui) playing: bool,
    /// Milliseconds between steps while playing.
    pub(in crate::gui) refresh_ms: u64,
    pub(in crate::gui) last_tick: Instant,
    /// Whether playing means one step per `refresh_ms` or a full frame budget
    /// of steps. "Run to +N" bursts regardless of this setting.
    pub(in crate::gui) pacing: Pacing,
    /// How many steps the "Run to +N" button should advance.
    pub(in crate::gui) run_to_steps: u64,
    /// The step number a pending "Run to +N" is heading for.
    pub(in crate::gui) run_to_target: Option<u64>,
    /// Whether playback was already running when "Run to +N" was pressed, so a
    /// finished run can leave `playing` the way it found it.
    pub(in crate::gui) playing_before_run_to: bool,
    /// Time accumulated over completed play segments.
    pub(in crate::gui) elapsed: Duration,
    /// Steps counted while the stopwatch was running.
    pub(in crate::gui) timed_steps: u64,
    /// Start of the current play segment, if one is running.
    pub(in crate::gui) play_start: Option<Instant>,
    /// Start of the window the live steps-per-second readout averages over.
    pub(in crate::gui) rate_window_start: Instant,
    /// Steps run since `rate_window_start`.
    pub(in crate::gui) rate_window_steps: u64,
    /// The live steps-per-second readout shown in the status bar.
    pub(in crate::gui) steps_per_s: f64,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            playing: false,
            refresh_ms: 100,
            last_tick: Instant::now(),
            pacing: Pacing::Interval,
            run_to_steps: 100,
            run_to_target: None,
            playing_before_run_to: false,
            elapsed: Duration::ZERO,
            timed_steps: 0,
            play_start: None,
            rate_window_start: Instant::now(),
            rate_window_steps: 0,
            steps_per_s: 0.0,
        }
    }
}

/// How the grid is drawn: zoom, colours, grid lines, and the 1D space-time
/// history that gives the classic Wolfram diagram its vertical axis.
pub(in crate::gui) struct ViewSettings {
    /// Pixel size of one cell.
    pub(in crate::gui) scale: usize,
    /// Explicit per-type colours, keyed by the interned symbol so lookups during
    /// painting are a plain integer hash rather than a `String` allocation.
    pub(in crate::gui) colors: HashMap<Spur, Color32>,
    pub(in crate::gui) palette: Vec<Color32>,
    /// Which preset `palette` came from (index into `theme::PALETTES`).
    pub(in crate::gui) palette_index: usize,
    /// Colours the loaded config asked for, kept so a palette change can put
    /// them back on top of the new automatic slots.
    pub(in crate::gui) config_colors: BTreeMap<String, String>,
    /// Scratch buffer for the shapes emitted while painting, reused every frame
    /// so painting does not reallocate.
    pub(in crate::gui) shape_buf: Vec<Shape>,
    pub(in crate::gui) inactive_color: Color32,
    pub(in crate::gui) show_grid_lines: bool,
    pub(in crate::gui) grid_line_color: Color32,
    /// Past 1D rows, oldest to newest, excluding the current one.
    ///
    /// Deliberately separate from `Grid1D`'s own history: the library keeps a
    /// short per-cell circular buffer that rules use for age/lookback, whereas
    /// the viewport wants whole-row snapshots going back hundreds of steps. A
    /// `VecDeque` avoids the O(n) memmove that `Vec::drain(0..k)` cost when the
    /// window overflows every step.
    pub(in crate::gui) history_1d: VecDeque<Vec<CellType>>,
    pub(in crate::gui) history_limit_1d: usize,
    /// Minimum rows to allocate in the 1D viewport, so scrollbars do not
    /// overlap the content.
    pub(in crate::gui) min_view_rows_1d: usize,
    /// The viewport's size on the last frame. The toolbar is drawn before the
    /// viewport, so "zoom to fit" reads last frame's value; one frame of lag
    /// is invisible.
    pub(in crate::gui) last_viewport_size: Option<egui::Vec2>,
    /// Show a tooltip with the cell under the mouse (position, type, age).
    pub(in crate::gui) inspector: bool,
    /// Overlay layers drawn over the cells.
    pub(in crate::gui) layers: LayerState,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            scale: 8,
            colors: HashMap::new(),
            palette: default_palette(),
            palette_index: 0,
            config_colors: BTreeMap::new(),
            shape_buf: Vec::new(),
            inactive_color: Color32::from_rgb(30, 30, 35),
            show_grid_lines: true,
            grid_line_color: Color32::from_rgb(60, 60, 70),
            history_1d: VecDeque::new(),
            history_limit_1d: 100,
            min_view_rows_1d: 3,
            last_viewport_size: None,
            inspector: true,
            layers: LayerState::default(),
        }
    }
}

/// Interactive editing of the grid: which tool is active and what to undo.
pub(in crate::gui) struct EditState {
    pub(in crate::gui) draw_mode: DrawMode,
    pub(in crate::gui) selected_draw_type: Option<CellType>,
    /// Stack of edit batches; each batch is `(index, previous_type)` pairs.
    pub(in crate::gui) undo_stack: Vec<Vec<(usize, CellType)>>,
    /// The batch currently being accumulated by a click-and-drag.
    pub(in crate::gui) current_paint_batch: Option<Vec<(usize, CellType)>>,
    /// Cells already written during the current stroke, so a brush that
    /// revisits a cell does not re-record it (and the check is not a scan).
    pub(in crate::gui) stroke_touched: HashSet<usize>,
    /// Brush diameter in cells (1 = a single cell).
    /// Brush range: 0 paints one cell, `n` paints the centre plus the
    /// `brush_shape` neighbourhood of range `n` (the same tables the rules use).
    pub(in crate::gui) brush: u8,
    /// Shape of the brush footprint.
    pub(in crate::gui) brush_shape: Neighborhood2D,
    /// Index into `patterns::PATTERNS` of the stamp to drop.
    pub(in crate::gui) stamp: usize,
    /// Random-fill drafts: share of cells to set, which type, the seed, and
    /// whether to clear the grid first.
    pub(in crate::gui) fill_density: f32,
    pub(in crate::gui) fill_type: Option<CellType>,
    pub(in crate::gui) fill_seed: u64,
    pub(in crate::gui) fill_clear: bool,
    /// Knob values before each Surprise me / Mutate rule / Apply genome, newest
    /// last, so Undo rule can walk back through them. Capped at
    /// [`RULE_UNDO_CAP`].
    pub(in crate::gui) rule_undo: Vec<Vec<(String, ParamValue)>>,
    /// How far Mutate rule nudges each knob, as a share of its range.
    pub(in crate::gui) mutate_sigma: f64,
}

/// Most rule undo entries kept.
pub(in crate::gui) const RULE_UNDO_CAP: usize = 16;

impl Default for EditState {
    fn default() -> Self {
        Self {
            draw_mode: DrawMode::Cycle,
            selected_draw_type: Some(CellType::inactive()),
            undo_stack: Vec::new(),
            current_paint_batch: None,
            stroke_touched: HashSet::new(),
            brush: 0,
            brush_shape: Neighborhood2D::Moore,
            stamp: 0,
            fill_density: 0.3,
            fill_type: None,
            fill_seed: 1,
            fill_clear: true,
            rule_undo: Vec::new(),
            mutate_sigma: 0.2,
        }
    }
}

/// A GIF export waiting on the options modal: where to write, and the frame
/// count and rate the user is editing.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct PendingExport {
    pub(in crate::gui) path: std::path::PathBuf,
    pub(in crate::gui) steps: u32,
    pub(in crate::gui) fps: u32,
    /// Whether the GIF loops forever (true) or plays once (false).
    pub(in crate::gui) looping: bool,
}

/// GIF export settings and the background thread doing the work.
#[derive(Default)]
pub(in crate::gui) struct ExportState {
    /// The file chosen and the options being edited in the export modal.
    /// `Some` is what tells `ui_export_modal` (in `panels::toolbar`) to draw
    /// it; it clears itself on Export or Cancel.
    pub(in crate::gui) pending: Option<PendingExport>,
    /// For 1D exports, stack each step as a row to produce a space-time image.
    pub(in crate::gui) with_history_1d: bool,
    pub(in crate::gui) total: usize,
    /// Shared counter the worker thread bumps so the UI can draw a progress bar.
    pub(in crate::gui) progress: Option<Arc<AtomicUsize>>,
    pub(in crate::gui) join: Option<std::thread::JoinHandle<Result<(), String>>>,
    pub(in crate::gui) message: Option<String>,
}

/// Per-type population series backing the statistics chart.
pub(in crate::gui) struct StatsState {
    pub(in crate::gui) history: BTreeMap<Spur, VecDeque<(u64, u64)>>,
    /// Which series are ticked for display.
    pub(in crate::gui) show: BTreeMap<Spur, bool>,
    /// How many samples the rolling window keeps.
    pub(in crate::gui) window_len: usize,
}

impl Default for StatsState {
    fn default() -> Self {
        Self {
            history: BTreeMap::new(),
            show: BTreeMap::new(),
            window_len: 300,
        }
    }
}

/// The rule editor's working copy of the rule, kept separate from the grid's
/// live rule until "Apply to grid" is pressed.
#[derive(Default)]
pub(in crate::gui) struct EditorState {
    pub(in crate::gui) rule_1d: Option<Rule1DEdit>,
    pub(in crate::gui) rule_2d: Option<Rule2DEdit>,
    pub(in crate::gui) error_msg: Option<String>,
    /// Types added through the editor, beyond those seen in rules or the grid.
    pub(in crate::gui) custom_types: BTreeSet<Spur>,
    pub(in crate::gui) new_type_name: String,
}

/// Which tab of the left control panel is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::gui) enum ControlTab {
    #[default]
    Scenario,
    Edit,
    Style,
    Stats,
}

/// Which tab of the right workbench panel is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::gui) enum WorkbenchTab {
    #[default]
    Rule,
    Model,
    Explore,
}

/// Window furniture: text scaling, the theme, which panels and tabs are open,
/// and the status line.
pub(in crate::gui) struct Chrome {
    pub(in crate::gui) font_scale: f32,
    /// Last `font_scale` actually pushed into the egui style, so the (fairly
    /// expensive) restyle only happens on frames where it changed.
    pub(in crate::gui) applied_font_scale: f32,
    pub(in crate::gui) base_text_styles: BTreeMap<egui::TextStyle, egui::FontId>,
    pub(in crate::gui) status_message: Option<String>,
    /// The theme the user chose.
    pub(in crate::gui) theme: ThemeChoice,
    /// The theme last pushed into egui (`None` before the first frame), so
    /// the restyle happens only when it changes.
    pub(in crate::gui) applied_theme: Option<ThemeChoice>,
    /// Whether the left control panel is open.
    pub(in crate::gui) left_open: bool,
    /// Whether the right workbench panel is open.
    pub(in crate::gui) right_open: bool,
    pub(in crate::gui) control_tab: ControlTab,
    pub(in crate::gui) workbench_tab: WorkbenchTab,
    /// Whether the keyboard-shortcut overlay is showing.
    pub(in crate::gui) show_shortcuts: bool,
    /// A loaded config whose `snapshot` is mid-run, waiting on the user to
    /// pick "Resume" or "Start from initial". `Some` is what tells
    /// `ui_snapshot_load_modal` (in `panels::toolbar`) to draw that modal;
    /// it clears itself once a choice is made.
    pub(in crate::gui) pending_snapshot_load: Option<PendingSnapshotLoad>,
    /// A one-off message waiting for the user to press OK, e.g. what a
    /// scenario load did with the Explore settings. `ui_notice_modal` (in
    /// `panels::toolbar`) draws it and clears it via
    /// [`Action::DismissNotice`](super::actions::Action::DismissNotice).
    pub(in crate::gui) notice: Option<Notice>,
}

/// A one-off message the user acknowledges with OK, e.g. what a load or a
/// save did with the Explore settings.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::gui) struct Notice {
    pub(in crate::gui) title: String,
    pub(in crate::gui) lines: Vec<String>,
}

/// A config loaded from disk whose run is mid-simulation, waiting for the
/// user to choose "Resume at step N" or "Start from initial" in the modal.
pub(in crate::gui) struct PendingSnapshotLoad {
    pub(in crate::gui) cfg: cella_lib::config::CellaConfig,
    /// File name, for the status message once resolved.
    pub(in crate::gui) name: String,
}

impl Chrome {
    /// Text styles are theme-independent, so either theme's style is a fine
    /// baseline to scale from.
    pub(in crate::gui) fn new(ctx: &egui::Context) -> Self {
        Self {
            font_scale: 1.0,
            applied_font_scale: 1.0,
            base_text_styles: ctx.style_of(egui::Theme::Dark).text_styles.clone(),
            status_message: None,
            theme: ThemeChoice::default(),
            applied_theme: None,
            left_open: true,
            right_open: true,
            control_tab: ControlTab::default(),
            workbench_tab: WorkbenchTab::default(),
            show_shortcuts: false,
            pending_snapshot_load: None,
            notice: None,
        }
    }
}

/// Pending values typed into form fields, not yet applied to anything.
pub(in crate::gui) struct Inputs {
    pub(in crate::gui) grid_width: usize,
    pub(in crate::gui) grid_height: usize,
    /// Wolfram code for the custom 1D builder, kept as text so a partially
    /// typed number is not clobbered.
    pub(in crate::gui) custom_code: String,
    pub(in crate::gui) custom_n: u8,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            grid_width: 50,
            grid_height: 30,
            custom_code: "30".into(),
            custom_n: 1,
        }
    }
}
