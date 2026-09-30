use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::render::color_for;
use cella_lib::*;
use egui::Color32;
use lasso2::Spur;

/// Build a stable palette mapping for GIF export.
///
/// Index 0 is reserved for the Inactive color. Subsequent indices are assigned
/// to distinct type names present in the provided `types` slice. Colors are
/// chosen from `colors` overrides when present, otherwise from hashing into the
/// provided `fallback_palette`.
fn build_palette_map(
    types: &[CellType],
    colors: &HashMap<Spur, Color32>,
    fallback_palette: &[Color32],
    inactive: Color32,
) -> (Vec<u8>, BTreeMap<Spur, u8>) {
    let mut color_table: Vec<u8> = Vec::with_capacity(256 * 3);

    // Index 0 = inactive
    color_table.extend_from_slice(&[inactive.r(), inactive.g(), inactive.b()]);

    // Assign indices 1.. to distinct types (excluding Inactive)
    let mut map: BTreeMap<Spur, u8> = BTreeMap::new();
    let mut next_index: u8 = 1;
    for ty in types {
        if ty.as_str() == INACTIVE {
            continue;
        }
        if map.contains_key(&ty.0) {
            continue;
        }
        let col = color_for(*ty, colors, fallback_palette, inactive);
        color_table.extend_from_slice(&[col.r(), col.g(), col.b()]);
        map.insert(ty.0, next_index);
        next_index = next_index.saturating_add(1);
        if next_index == 0 {
            break;
        } // avoid overflow; unlikely with few types
    }

    // Fill the rest of the 256-color table with repeats of fallback palette
    while color_table.len() < 256 * 3 {
        let idx = ((color_table.len() / 3) - 1) % fallback_palette.len().max(1);
        let c = fallback_palette
            .get(idx)
            .copied()
            .unwrap_or(Color32::LIGHT_BLUE);
        color_table.extend_from_slice(&[c.r(), c.g(), c.b()]);
    }

    (color_table, map)
}

/// Palette index for a type: 0 for Inactive, otherwise the assigned slot.
#[inline]
fn pal_index_of(index_map: &BTreeMap<Spur, u8>, ty: CellType) -> u8 {
    if ty == CellType::inactive() {
        0
    } else {
        index_map.get(&ty.0).copied().unwrap_or(1)
    }
}

/// Paint one logical cell as a `scale`×`scale` block of palette indices.
///
/// Each scanline of the block is a contiguous run, so this is a memset per row
/// rather than a per-pixel store.
#[inline]
fn blit_cell(buf: &mut [u8], stride: usize, cell_x: usize, cell_y: usize, scale: usize, value: u8) {
    let x0 = cell_x * scale;
    for dy in 0..scale {
        let row = (cell_y * scale + dy) * stride;
        buf[row + x0..row + x0 + scale].fill(value);
    }
}

/// Distinct cell types the export needs colors for: everything currently on the
/// grid, plus every type any subrule can produce, plus every type an attached
/// external model declares.
///
/// Including the rule's and model's types matters because a state that is
/// absent on frame 0 (a wildfire's Burning and Burned, say) but appears later
/// would otherwise fall back to palette slot 1 and render in the wrong color
/// for the rest of the animation.
fn types_for_export(
    on_grid: impl Iterator<Item = CellType>,
    from_rules: impl Iterator<Item = CellType>,
    from_model: impl Iterator<Item = CellType>,
) -> Vec<CellType> {
    let mut set: BTreeMap<Spur, CellType> = BTreeMap::new();
    for ty in on_grid.chain(from_rules).chain(from_model) {
        set.insert(ty.0, ty);
    }
    set.into_values().collect()
}

/// Everything a GIF export needs besides the grid itself.
pub struct GifExport<'a> {
    /// Destination file.
    pub path: std::path::PathBuf,
    /// Number of frames (simulation steps) to write.
    pub steps: usize,
    /// Playback rate.
    pub fps: u32,
    /// Loop forever when true. When false no loop marker is written, so the
    /// GIF plays once and stops on its last frame.
    pub looping: bool,
    /// Pixels per cell.
    pub scale: u16,
    /// User-assigned colors, keyed by interned type name.
    pub colors: &'a HashMap<Spur, Color32>,
    /// Fallback palette for types with no assigned color.
    pub palette: &'a [Color32],
    /// Color for the Inactive state.
    pub inactive: Color32,
    /// Frames-written counter, polled by the UI to drive a progress bar.
    pub progress: Option<&'a Arc<AtomicUsize>>,
}

/// Export a 2D grid to an animated GIF. Colors match the GUI mapping,
/// including the configurable Inactive color.
pub fn export_gif_2d(
    grid: &mut Grid2D,
    opts: &GifExport<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    use gif::{Encoder, Frame};
    let &GifExport {
        steps,
        fps,
        scale,
        colors,
        palette,
        inactive,
        progress,
        ..
    } = opts;
    let path = opts.path.clone();
    let w = (grid.width as u16).saturating_mul(scale);
    let h = (grid.height as u16).saturating_mul(scale);
    let scale = scale as usize;
    let stride = w as usize;

    let types = types_for_export(
        (0..grid.width * grid.height).map(|i| grid.cell_type(i)),
        grid.rule
            .subrules
            .iter()
            .flat_map(|s| [s.current_type, s.criteria_type, s.output_type]),
        // External models (e.g. wildfire) declare states that may not be on
        // the grid yet.
        grid.model
            .iter()
            .flat_map(|m| m.declared_types().into_iter()),
    );
    let (color_table, index_map) = build_palette_map(&types, colors, palette, inactive);

    let mut file = std::fs::File::create(path)?;
    let mut encoder = Encoder::new(&mut file, w, h, &color_table)?;
    if opts.looping {
        encoder.set_repeat(gif::Repeat::Infinite)?;
    }
    let delay_cs = (100.0 / (fps.max(1) as f32)).round() as u16;

    // Reused across frames and lent to the encoder, avoiding a fresh zeroed
    // allocation of w*h bytes on every single frame.
    let mut buf = vec![0u8; stride * (h as usize)];

    for i in 0..steps {
        for y in 0..grid.height {
            for x in 0..grid.width {
                let pal = pal_index_of(&index_map, grid.cell_type(y * grid.width + x));
                blit_cell(&mut buf, stride, x, y, scale, pal);
            }
        }
        let frame = Frame {
            width: w,
            height: h,
            delay: delay_cs,
            buffer: std::borrow::Cow::Borrowed(&buf),
            ..Default::default()
        };
        encoder.write_frame(&frame)?;
        if let Some(p) = progress {
            p.store(i + 1, Ordering::Relaxed);
        }
        grid.step();
    }

    Ok(())
}

/// Export a 1D grid to an animated GIF. Colors match the GUI mapping,
/// including the configurable Inactive color.
pub fn export_gif_1d(
    grid: &mut Grid1D,
    opts: &GifExport<'_>,
    history_rows: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    use gif::{Encoder, Frame};
    let &GifExport {
        steps,
        fps,
        scale,
        colors,
        palette,
        inactive,
        progress,
        ..
    } = opts;
    let path = opts.path.clone();
    let w = (grid.width as u16).saturating_mul(scale);
    let total_planned_rows = 1usize + steps;
    let display_rows: usize = match history_rows {
        Some(maxr) => maxr.max(1).min(total_planned_rows),
        None => 1,
    };
    let h = (display_rows as u16).saturating_mul(scale);
    let scale = scale as usize;
    let stride = w as usize;

    let types = types_for_export(
        (0..grid.width).map(|i| grid.cell_type(i)),
        grid.rule
            .subrules
            .iter()
            .flat_map(|s| [s.current_type, s.criteria_type, s.output_type]),
        // A 1D grid has no external model, so nothing more to declare.
        std::iter::empty(),
    );
    let (color_table, index_map) = build_palette_map(&types, colors, palette, inactive);

    let mut file = std::fs::File::create(path)?;
    let mut encoder = Encoder::new(&mut file, w, h, &color_table)?;
    if opts.looping {
        encoder.set_repeat(gif::Repeat::Infinite)?;
    }

    let delay_cs = (100.0 / (fps.max(1) as f32)).round() as u16;

    // Local rolling history of prior rows (excluding current), stored as palette
    // indices so each frame is a straight blit with no per-pixel type lookup.
    let hist_cap = display_rows.saturating_sub(1);
    let mut history: VecDeque<Vec<u8>> = VecDeque::with_capacity(hist_cap);
    let mut buf = vec![0u8; stride * (h as usize)];

    for i in 0..steps {
        buf.fill(0);
        // Draw history rows (from oldest within window to newest)
        for (j, row) in history.iter().enumerate() {
            for (x, &pal) in row.iter().enumerate() {
                blit_cell(&mut buf, stride, x, j, scale, pal);
            }
        }

        // Draw current row at the bottom of the visible window
        let cur_y = history.len();
        let mut row_now: Vec<u8> = Vec::with_capacity(grid.width);
        for x in 0..grid.width {
            let pal = pal_index_of(&index_map, grid.cell_type(x));
            row_now.push(pal);
            blit_cell(&mut buf, stride, x, cur_y, scale, pal);
        }

        let frame = Frame {
            width: w,
            height: h,
            delay: delay_cs,
            buffer: std::borrow::Cow::Borrowed(&buf),
            ..Default::default()
        };
        encoder.write_frame(&frame)?;
        if let Some(p) = progress {
            p.store(i + 1, Ordering::Relaxed);
        }

        // After writing the frame, push the current row into history and cap length
        if hist_cap > 0 {
            history.push_back(row_now);
            while history.len() > hist_cap {
                history.pop_front();
            }
        }

        grid.step();
    }

    Ok(())
}

use crate::gui::app::{CellaApp, Dim};
use crate::gui::render::color_to_hex;
use crate::gui::state::{Notice, PendingExport};
use cella_lib::config::CellaConfig;
use cella_lib::types::interner;
use rfd::FileDialog;

/// Frame count the export modal starts with: the "Run to +N" box, kept inside
/// what the modal's steps field accepts.
pub(in crate::gui) fn default_export_steps(run_to_steps: u64) -> u32 {
    run_to_steps.clamp(1, MAX_EXPORT_STEPS as u64) as u32
}

/// Frame rate the export modal starts with: the playback speed
/// (`1000 / refresh_ms` steps per second), so the GIF plays as fast as the
/// live view did. GIF frame delays are whole centiseconds, so anything above
/// 50 fps cannot be represented and the range stops there.
pub(in crate::gui) fn default_export_fps(refresh_ms: u64) -> u32 {
    let fps = (1000.0 / refresh_ms.max(1) as f64).round() as u32;
    fps.clamp(1, MAX_EXPORT_FPS)
}

/// Upper limit of the modal's steps field.
pub(in crate::gui) const MAX_EXPORT_STEPS: u32 = 10_000;
/// Upper limit of the modal's fps field (one frame per centisecond).
pub(in crate::gui) const MAX_EXPORT_FPS: u32 = 50;

/// How far along an export is, from 0.0 to 1.0. A zero total counts as
/// "nothing done yet" rather than dividing by zero.
pub(in crate::gui) fn progress_fraction(done: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f32 / total as f32).min(1.0)
    }
}

impl CellaApp {
    /// Every explicit colour override plus the Inactive background colour,
    /// as `#rrggbb` strings — what a save file's `colors` block holds.
    pub(in crate::gui) fn color_map_for_save(&self) -> BTreeMap<String, String> {
        let mut colors: BTreeMap<String, String> = self
            .view
            .colors
            .iter()
            .map(|(&spur, &c)| (interner().resolve(&spur).to_string(), color_to_hex(c)))
            .collect();
        colors.insert(INACTIVE.to_string(), color_to_hex(self.view.inactive_color));
        colors
    }

    /// What Save writes: the grid as a config (with its snapshot past step
    /// 0), the colours, and any Explore blocks the session used; plus the
    /// notice to show when an Explore block was written.
    pub(in crate::gui) fn config_for_save(&self) -> Option<(CellaConfig, Option<Notice>)> {
        let initial = self.scenario.initial_state.as_ref()?;
        let colors = self.color_map_for_save();
        let mut cfg = match self.scenario.dim? {
            Dim::D1 => CellaConfig::save_1d(initial, self.scenario.d1.as_ref()?, colors),
            Dim::D2 => CellaConfig::save_2d(initial, self.scenario.d2.as_ref()?, colors),
        };
        let ensemble = self.ensemble_block_for_save();
        let evolve = self.evolve_block_for_save();
        let mut lines = Vec::new();
        if let Some(e) = &ensemble {
            lines.push(format!("Ensemble settings: {} members, {} genes.", e.members, e.genes.len()));
            if self.explore.base_ensemble.is_some() && !self.explore.kept_ensemble.is_empty() {
                lines.push(format!("Kept from the loaded file: {}.", self.explore.kept_ensemble.join("; ")));
            }
        }
        if let Some(e) = &evolve {
            lines.push(format!(
                "Evolve settings: population {}, {} generations, {} genes.",
                e.population, e.generations, e.genes.len()
            ));
            if self.explore.base_evolve.is_some() && !self.explore.kept_evolve.is_empty() {
                lines.push(format!("Kept from the loaded file: {}.", self.explore.kept_evolve.join("; ")));
            }
        }
        cfg.set_explore_blocks(ensemble, evolve);
        let notice = (!lines.is_empty()).then(|| Notice { title: "Explore settings saved".into(), lines });
        Some((cfg, notice))
    }

    /// Save the current scenario as a config: dims, rule, model, seed and
    /// colours, plus a `snapshot` of the run in progress if it has stepped
    /// past 0, plus any Explore blocks the session used. The same file loads
    /// back through "Load scenario".
    pub(in crate::gui) fn save_final_state(&mut self) {
        let Some((cfg, notice)) = self.config_for_save() else {
            return;
        };
        match FileDialog::new()
            .set_directory(std::env::current_dir().unwrap_or_default())
            .set_file_name("snapshot.json")
            .save_file()
        {
            Some(path) => match cfg.to_file_pretty(&path) {
                Ok(()) => {
                    self.set_status(format!("Saved {}", path.display()));
                    self.chrome.notice = notice;
                }
                Err(e) => self.set_status(format!("Failed to save: {e}")),
            },
            None => self.report_no_file_chosen("save path"),
        }
    }
    /// Start a GIF export: ask for the file, then hand over to the options
    /// modal (`ui_export_modal`), which asks for steps and fps.
    ///
    /// Only the path is chosen here; the export itself starts when the modal
    /// answers [`Action::ConfirmExportGif`](crate::gui::actions::Action).
    pub(in crate::gui) fn export_gif_dialog(&mut self) {
        if self.export.join.is_some() {
            return;
        }
        let Some(path) = FileDialog::new()
            .set_directory(std::env::current_dir().unwrap_or_default())
            .add_filter("gif", &["gif"])
            .set_file_name("cella.gif")
            .save_file()
        else {
            self.report_no_file_chosen("GIF path");
            return;
        };
        self.begin_export_options(path);
    }

    /// Open the options modal for `path`, prefilled from the playback
    /// controls: "Run to +N" for the steps, the speed slider for the fps.
    pub(in crate::gui) fn begin_export_options(&mut self, path: std::path::PathBuf) {
        self.export.pending = Some(PendingExport {
            path,
            steps: default_export_steps(self.playback.run_to_steps),
            fps: default_export_fps(self.playback.refresh_ms),
            looping: true,
        });
    }

    /// The modal's "Cancel": forget the chosen path and options.
    pub(in crate::gui) fn cancel_export(&mut self) {
        self.export.pending = None;
    }

    /// `(frames written, frames planned)` while an export is running.
    pub(in crate::gui) fn export_progress(&self) -> Option<(usize, usize)> {
        let p = self.export.progress.as_ref()?;
        Some((p.load(Ordering::Relaxed), self.export.total))
    }

    /// The modal's "Export": take the pending path and options and start the
    /// background thread. Runs the export off the UI thread and shows a
    /// progress bar in the toolbar and status bar (via `export_progress`).
    pub(in crate::gui) fn confirm_export(&mut self) {
        let Some(PendingExport { path, steps, fps, looping }) = self.export.pending.take() else {
            return;
        };
        if self.export.join.is_some() {
            return;
        }
        {
            let steps = steps.max(1) as usize;
            let fps = fps.max(1);
            let scale = self.view.scale as u16;
            let colors = self.view.colors.clone();
            let palette = self.view.palette.clone();
            let inactive = self.inactive_color();
            let progress = Arc::new(AtomicUsize::new(0));
            self.export.total = steps;
            self.export.progress = Some(progress.clone());
            self.set_status(format!("Exporting GIF: {} frames @ {} fps", steps, fps));
            // `GifExport` borrows the color tables, so it is built inside the worker
            // thread that owns the clones.
            match self.scenario.dim {
                Some(Dim::D1) => {
                    if let Some(g) = &self.scenario.d1 {
                        let mut grid_clone = g.clone();
                        let history_opt = if self.export.with_history_1d {
                            Some(self.view.history_limit_1d)
                        } else {
                            None
                        };
                        let handle = std::thread::spawn(move || {
                            let opts = GifExport {
                                path,
                                steps,
                                fps,
                                looping,
                                scale,
                                colors: &colors,
                                palette: &palette,
                                inactive,
                                progress: Some(&progress),
                            };
                            export_gif_1d(&mut grid_clone, &opts, history_opt)
                                .map_err(|e| e.to_string())
                        });
                        self.export.join = Some(handle);
                    }
                }
                Some(Dim::D2) => {
                    if let Some(g) = &self.scenario.d2 {
                        let mut grid_clone = g.clone();
                        let handle = std::thread::spawn(move || {
                            let opts = GifExport {
                                path,
                                steps,
                                fps,
                                looping,
                                scale,
                                colors: &colors,
                                palette: &palette,
                                inactive,
                                progress: Some(&progress),
                            };
                            export_gif_2d(&mut grid_clone, &opts).map_err(|e| e.to_string())
                        });
                        self.export.join = Some(handle);
                    }
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::actions::Action;
    use crate::gui::sim::tests::test_app;

    #[test]
    fn color_map_for_save_includes_overrides_and_inactive_as_hex() {
        let mut app = test_app();
        app.load_demo_life();
        app.apply_action(Action::SetTypeColor(
            CellType::from("Alive"),
            Color32::from_rgb(0xaa, 0xbb, 0xcc),
        ));
        app.apply_action(Action::SetInactiveColor(Color32::from_rgb(1, 2, 3)));
        let colors = app.color_map_for_save();
        assert_eq!(colors.get("Alive").map(String::as_str), Some("#aabbcc"));
        assert_eq!(colors.get(INACTIVE).map(String::as_str), Some("#010203"));
    }

    /// Build a save file the way the Save button does (`color_map_for_save` +
    /// `CellaConfig::save_2d`), then load it back through the ordinary
    /// scenario loader: the custom colour must survive the round trip.
    #[test]
    fn save_then_load_keeps_the_custom_colours() {
        let mut app = test_app();
        app.load_demo_life();
        let red = Color32::from_rgb(0x11, 0x22, 0x33);
        app.apply_action(Action::SetTypeColor(CellType::from("Alive"), red));
        let colors = app.color_map_for_save();
        let initial = app.scenario.initial_state.clone().expect("reset target");
        let g = app.scenario.d2.as_ref().expect("2D grid loaded");
        let cfg = CellaConfig::save_2d(&initial, g, colors);
        let path = std::env::temp_dir().join(format!(
            "cella_save_load_test_{}.json",
            std::process::id()
        ));
        cfg.to_file_pretty(&path).expect("write test config");
        app.load_config_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(app.color_of(&CellType::from("Alive")), red);
    }

    /// A model's declared states that are absent from frame 0 (a wildfire's
    /// Burning and Burned) must still get their own palette slot and color.
    #[test]
    fn model_declared_types_get_palette_slots_and_colours() {
        use cella_lib::wildfire::{FuelClass, WildfireEnv, WildfireModel, WildfireParams};
        let mut app = test_app();
        app.load_demo_life();
        let params = WildfireParams {
            seed: 7,
            p0: 0.3,
            fuels: vec![FuelClass { name: "Forest".to_string(), veg_factor: 1.0 }],
            wind_speed: 1.0,
            wind_from_deg: 270.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 1,
            spotting: None,
            burning_name: None,
            burned_name: None,
            spread: "bernoulli".into(),
            arrival_jitter: 0.2,
            wind_law: "exponential".into(),
        };
        let mut grid = app.scenario.d2.take().expect("2D grid");
        grid.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
            .expect("model validates");
        let burned = CellType::from("BurnedOut");
        let red = Color32::from_rgb(200, 10, 20);
        let mut colors = HashMap::new();
        colors.insert(burned.0, red);
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cella_palette_test_{}.gif", std::process::id()));
        let opts = GifExport {
            path: path.clone(),
            steps: 1,
            fps: 10,
            looping: true,
            scale: 1,
            colors: &colors,
            palette: &[Color32::from_rgb(1, 2, 3)],
            inactive: Color32::BLACK,
            progress: None,
        };
        // The GIF's global palette is what the fix changes: Burned must be in it.
        export_gif_2d(&mut grid, &opts).expect("export");
        let bytes = std::fs::read(&path).expect("gif written");
        let _ = std::fs::remove_file(&path);
        let has_red = bytes.windows(3).any(|w| w == [200, 10, 20]);
        assert!(has_red, "Burned's colour is in the palette");

        let types = types_for_export(
            std::iter::empty(),
            std::iter::empty(),
            grid.model.iter().flat_map(|m| m.declared_types().into_iter()),
        );
        assert!(types.contains(&burned));
        let (_, map) = build_palette_map(&types, &colors, &[], Color32::BLACK);
        assert!(map.contains_key(&burned.0));
    }

    #[test]
    fn export_defaults_follow_the_playback_controls() {
        assert_eq!(default_export_steps(100), 100);
        assert_eq!(default_export_steps(0), 1);
        assert_eq!(default_export_steps(u64::MAX), MAX_EXPORT_STEPS);
        assert_eq!(default_export_fps(100), 10);
        assert_eq!(default_export_fps(1000), 1);
        assert_eq!(default_export_fps(5000), 1, "floored at 1 fps");
        assert_eq!(default_export_fps(40), 25);
        assert_eq!(default_export_fps(1), MAX_EXPORT_FPS, "clamped to 50");
        assert_eq!(default_export_fps(0), MAX_EXPORT_FPS, "0 ms is treated as 1");
    }

    #[test]
    fn progress_fraction_is_safe_and_bounded() {
        assert_eq!(progress_fraction(0, 0), 0.0);
        assert_eq!(progress_fraction(5, 10), 0.5);
        assert_eq!(progress_fraction(12, 10), 1.0);
    }

    #[test]
    fn export_options_are_prefilled_then_cancel_and_confirm_clear_them() {
        let mut app = test_app();
        app.load_demo_life();
        app.playback.run_to_steps = 42;
        app.playback.refresh_ms = 200;
        let path = std::env::temp_dir().join(format!("cella_confirm_test_{}.gif", std::process::id()));
        app.begin_export_options(path.clone());
        assert!(app.export.pending.as_ref().unwrap().looping, "looping is on by default");
        assert_eq!(
            app.export.pending,
            Some(PendingExport { path: path.clone(), steps: 42, fps: 5, looping: true })
        );

        app.apply_action(Action::CancelExportGif);
        assert!(app.export.pending.is_none());
        assert!(app.export.join.is_none(), "cancel starts nothing");

        app.begin_export_options(path.clone());
        app.export.pending.as_mut().unwrap().steps = 3;
        app.apply_action(Action::ConfirmExportGif);
        assert!(app.export.pending.is_none());
        assert_eq!(app.export.total, 3, "the edited steps were used");
        assert!(app.export_progress().is_some());
        app.export.join.take().expect("export thread started").join().unwrap().unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);

        // Confirm with nothing pending is a no-op.
        app.export.progress = None;
        app.apply_action(Action::ConfirmExportGif);
        assert!(app.export.join.is_none());
    }

    #[test]
    fn looping_flag_controls_the_netscape_loop_extension() {
        let mut app = test_app();
        app.load_demo_life();
        let colors = HashMap::new();
        for looping in [true, false] {
            let mut grid = app.scenario.d2.clone().expect("2D grid");
            let path = std::env::temp_dir()
                .join(format!("cella_loop_test_{}_{}.gif", std::process::id(), looping));
            let opts = GifExport {
                path: path.clone(),
                steps: 2,
                fps: 10,
                looping,
                scale: 1,
                colors: &colors,
                palette: &[Color32::from_rgb(1, 2, 3)],
                inactive: Color32::BLACK,
                progress: None,
            };
            export_gif_2d(&mut grid, &opts).expect("export");
            let bytes = std::fs::read(&path).expect("gif written");
            let _ = std::fs::remove_file(&path);
            let has_marker = bytes.windows(11).any(|w| w == b"NETSCAPE2.0");
            assert_eq!(has_marker, looping, "looping={looping}");
        }
    }
}
