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

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use super::app::{Dim, DrawMode};
use super::panels::rule_edit_model::{Rule1DEdit, Rule2DEdit};
use super::render::default_palette;
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

/// Play/pause, the "Run to +N" target, and the stopwatch behind the
/// "Avg ms/step" readout in the status bar.
pub(in crate::gui) struct Playback {
    pub(in crate::gui) playing: bool,
    /// Milliseconds between steps while playing.
    pub(in crate::gui) refresh_ms: u64,
    pub(in crate::gui) last_tick: Instant,
    /// How many steps the "Run to +N" button should advance.
    pub(in crate::gui) run_to_steps: u64,
    /// The step number a pending "Run to +N" is heading for.
    pub(in crate::gui) run_to_target: Option<u64>,
    /// Time accumulated over completed play segments.
    pub(in crate::gui) elapsed: Duration,
    /// Steps counted while the stopwatch was running.
    pub(in crate::gui) timed_steps: u64,
    /// Start of the current play segment, if one is running.
    pub(in crate::gui) play_start: Option<Instant>,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            playing: false,
            refresh_ms: 100,
            last_tick: Instant::now(),
            run_to_steps: 100,
            run_to_target: None,
            elapsed: Duration::ZERO,
            timed_steps: 0,
            play_start: None,
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
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
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
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            draw_mode: DrawMode::Cycle,
            selected_draw_type: Some(CellType::inactive()),
            undo_stack: Vec::new(),
            current_paint_batch: None,
        }
    }
}

/// GIF export settings and the background thread doing the work.
pub(in crate::gui) struct ExportState {
    pub(in crate::gui) steps: u32,
    pub(in crate::gui) fps: u32,
    /// For 1D exports, stack each step as a row to produce a space-time image.
    pub(in crate::gui) with_history_1d: bool,
    pub(in crate::gui) total: usize,
    /// Shared counter the worker thread bumps so the UI can draw a progress bar.
    pub(in crate::gui) progress: Option<Arc<AtomicUsize>>,
    pub(in crate::gui) join: Option<std::thread::JoinHandle<Result<(), String>>>,
    pub(in crate::gui) message: Option<String>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            steps: 300,
            fps: 12,
            with_history_1d: false,
            total: 0,
            progress: None,
            join: None,
            message: None,
        }
    }
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
pub(in crate::gui) struct EditorState {
    pub(in crate::gui) rule_1d: Option<Rule1DEdit>,
    pub(in crate::gui) rule_2d: Option<Rule2DEdit>,
    pub(in crate::gui) error_msg: Option<String>,
    /// Types added through the editor, beyond those seen in rules or the grid.
    pub(in crate::gui) custom_types: BTreeSet<Spur>,
    pub(in crate::gui) new_type_name: String,
    /// Whether the right-hand rule editor panel is open.
    pub(in crate::gui) visible: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            rule_1d: None,
            rule_2d: None,
            error_msg: None,
            custom_types: BTreeSet::new(),
            new_type_name: String::new(),
            visible: true,
        }
    }
}

/// Window furniture: text scaling and the status line.
pub(in crate::gui) struct Chrome {
    pub(in crate::gui) font_scale: f32,
    /// Last `font_scale` actually pushed into the egui style, so the (fairly
    /// expensive) restyle only happens on frames where it changed.
    pub(in crate::gui) applied_font_scale: f32,
    pub(in crate::gui) base_text_styles: BTreeMap<egui::TextStyle, egui::FontId>,
    pub(in crate::gui) status_message: Option<String>,
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
