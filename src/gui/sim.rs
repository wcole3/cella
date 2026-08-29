//! Driving the simulation forward and recording what happened.
//!
//! This is the clock of the application: it decides *when* a step runs,
//! performs the step, and appends the resulting population counts to the
//! statistics series the chart reads.
//!
//! Each frame takes exactly one of two paths. Ordinary playback is *paced*:
//! at most one step per `refresh_ms`, because that interval is the animation
//! speed the user chose. A "Run to +N" (and, later, "Max speed" playback) is a
//! *burst*: it runs as many steps as fit in [`RUN_TO_FRAME_BUDGET`], so
//! throughput is set by the engine rather than by the frame rate. The clock
//! lives in [`CellaApp::tick_play`]; the stepping itself lives in the
//! time-free [`CellaApp::run_to_batch`], which is what the tests drive.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::app::{CellaApp, Dim};
use super::state::Pacing;
use cella_lib::types::interner;
use cella_lib::*;
use lasso2::Spur;

/// How long one frame may spend running steps during a burst, roughly half a
/// 60 fps frame, so the UI still gets its turn and stays responsive.
pub(in crate::gui) const RUN_TO_FRAME_BUDGET: Duration = Duration::from_millis(8);

/// Absolute backstop on steps run in a single frame, so a trivial rule on a
/// coarse system clock cannot make the burst loop effectively unbounded.
pub(in crate::gui) const RUN_TO_MAX_STEPS_PER_FRAME: u32 = 1_000_000;

/// How many steps a burst runs between readings of the clock. Small enough
/// that the budget is honoured, large enough that `Instant::now()` is noise.
pub(in crate::gui) const RUN_TO_CHUNK: u32 = 32;

impl CellaApp {
    /// Advance the automaton one step and maintain the 1D history buffer.
    pub(in crate::gui) fn step_once(&mut self) {
        // Snapshot the timer before stepping so we can measure elapsed time
        if let Some(start) = self.playback.play_start {
            self.playback.elapsed += start.elapsed();
            self.playback.play_start = Some(Instant::now());
        }
        match self.scenario.dim {
            Some(Dim::D1) => {
                if let Some(g) = &mut self.scenario.d1 {
                    // Push the current row onto the viewport's space-time history before
                    // stepping. Recycle the row buffer that falls out of the window instead
                    // of allocating a fresh `Vec` every step.
                    let mut row = if self.view.history_1d.len() >= self.view.history_limit_1d {
                        let mut recycled = self.view.history_1d.pop_front().unwrap_or_default();
                        recycled.clear();
                        recycled
                    } else {
                        Vec::new()
                    };
                    row.reserve(g.width);
                    row.extend((0..g.width).map(|x| g.cell_type(x)));
                    self.view.history_1d.push_back(row);
                    while self.view.history_1d.len() > self.view.history_limit_1d {
                        self.view.history_1d.pop_front();
                    }
                    g.step();
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &mut self.scenario.d2 {
                    g.step();
                }
            }
            None => {}
        }
        // Count this step for timing average (only when timer is running)
        if self.playback.play_start.is_some() {
            self.playback.timed_steps += 1;
        }
        // record stats after a successful step
        self.stats_record_step();
    }
    /// Current step number from the loaded grid, or 0 when none loaded.
    pub(in crate::gui) fn current_step(&self) -> u64 {
        match self.scenario.dim {
            Some(Dim::D1) => self.scenario.d1.as_ref().map(|g| g.step).unwrap_or(0),
            Some(Dim::D2) => self.scenario.d2.as_ref().map(|g| g.step).unwrap_or(0),
            None => 0,
        }
    }
    /// Start a "Run to +N", or extend one that is already running: aim
    /// `run_to_steps` past the current step and burst until we get there.
    ///
    /// `playing_before_run_to` is only saved when no run is pending. Pressing
    /// the button a second time to extend a run must keep the value from the
    /// first press, because that is the state the user was actually in — after
    /// the first press `playing` is a value this button forced on itself, and
    /// saving it would leave a run started from a pause animating forever.
    pub(in crate::gui) fn start_run_to(&mut self) {
        let target = self
            .current_step()
            .saturating_add(self.playback.run_to_steps);
        if self.playback.run_to_target.is_none() {
            // Remember how playback was set up so the finished run can
            // restore it instead of always stopping.
            self.playback.playing_before_run_to = self.playback.playing;
        }
        self.playback.run_to_target = Some(target);
        self.playback.playing = true; // ensure stepping
        if self.playback.play_start.is_none() {
            self.playback.play_start = Some(Instant::now());
        }
        self.set_status(format!("Running to {}", target));
    }
    /// The step number this frame should burst towards, or `None` when
    /// playback is paced (or stopped) and so runs at most one step per frame.
    ///
    /// A pending "Run to +N" bursts towards its target; unbounded playback has
    /// no target, so it bursts towards `u64::MAX` and simply never arrives.
    pub(in crate::gui) fn burst_target(&self) -> Option<u64> {
        match self.playback.run_to_target {
            Some(target) => Some(target),
            None if self.playback.playing && self.playback.pacing == Pacing::Unbounded => {
                Some(u64::MAX)
            }
            None => None,
        }
    }
    /// Step towards `target` up to `max_steps` times, and report how many steps
    /// actually ran (fewer than `max_steps` means the target was reached).
    ///
    /// Reads no clock at all: [`CellaApp::tick_play`] owns the time budget and
    /// calls this in small chunks, which is what makes the loop unit-testable.
    pub(in crate::gui) fn run_to_batch(&mut self, target: u64, max_steps: u32) -> u32 {
        let mut done = 0;
        while done < max_steps && self.current_step() < target {
            self.step_once();
            done += 1;
        }
        done
    }
    /// Play loop: exactly one of two branches steps per frame — a time-budgeted
    /// burst, or a single step paced by the refresh interval.
    pub(in crate::gui) fn tick_play(&mut self) {
        let run_to = self.playback.run_to_target;
        if let Some(target) = self.burst_target() {
            // Burst: keep stepping until the frame's time budget is spent. The
            // clock is read once per chunk rather than once per step, which on
            // any realistic grid is noise next to the steps themselves.
            let deadline = Instant::now() + RUN_TO_FRAME_BUDGET;
            let mut done = 0u32;
            while Instant::now() < deadline && done < RUN_TO_MAX_STEPS_PER_FRAME {
                let n = self.run_to_batch(target, RUN_TO_CHUNK);
                done += n;
                if n < RUN_TO_CHUNK {
                    break; // reached the target
                }
            }
            if run_to.is_some() && self.current_step() >= target {
                // The run finished: hand `playing` back the way "Run to +N"
                // found it, and close the timer segment only if it stops here.
                self.playback.run_to_target = None;
                self.playback.playing = self.playback.playing_before_run_to;
                if !self.playback.playing
                    && let Some(start) = self.playback.play_start.take()
                {
                    self.playback.elapsed += start.elapsed();
                }
            }
            return;
        }
        // Paced: step at most once per refresh interval under play.
        let now = Instant::now();
        let interval = Duration::from_millis(self.playback.refresh_ms);
        if self.playback.playing && now.duration_since(self.playback.last_tick) >= interval {
            self.playback.last_tick = now;
            self.step_once();
        }
    }
    /// Internal: clear and initialize statistics history/toggles from current grid.
    /// Also resets the simulation timer.
    pub(in crate::gui) fn stats_clear_and_init(&mut self) {
        self.playback.elapsed = Duration::ZERO;
        self.playback.timed_steps = 0;
        self.playback.play_start = None;
        self.stats.history.clear();
        self.stats.show.clear();
        let inactive = CellType::inactive().0;
        let (mut entries, step): (Vec<(Spur, u64)>, u64) = match self.scenario.dim {
            Some(Dim::D1) => match &self.scenario.d1 {
                Some(g) => (
                    g.counts_current.iter().map(|(k, v)| (*k, *v)).collect(),
                    g.step,
                ),
                None => return,
            },
            Some(Dim::D2) => match &self.scenario.d2 {
                Some(g) => (
                    g.counts_current.iter().map(|(k, v)| (*k, *v)).collect(),
                    g.step,
                ),
                None => return,
            },
            None => return,
        };
        if !entries.iter().any(|(k, _)| *k == inactive) {
            entries.push((inactive, 0));
        }
        // Order: Inactive first, then by name
        entries.sort_by(|a, b| {
            let (a, b) = (interner().resolve(&a.0), interner().resolve(&b.0));
            (a != INACTIVE).cmp(&(b != INACTIVE)).then_with(|| a.cmp(b))
        });
        // Default visibility: first 9 active types (Inactive off by default)
        let mut shown_left = 9usize;
        for (k, c) in entries {
            let show = if k == inactive {
                false
            } else if shown_left > 0 {
                shown_left -= 1;
                true
            } else {
                false
            };
            self.stats.show.insert(k, show);
            self.stats
                .history
                .insert(k, VecDeque::from(vec![(step, c)]));
        }
    }
    /// Internal: after stepping, append counts for each known type and cap window.
    ///
    /// The series maps are moved out of `self` for the duration so the grid's
    /// `counts_current` can be read in place rather than cloned every single step.
    pub(in crate::gui) fn stats_record_step(&mut self) {
        let mut history = std::mem::take(&mut self.stats.history);
        let mut show = std::mem::take(&mut self.stats.show);
        let window = self.stats.window_len.max(1);
        let inactive = CellType::inactive().0;

        let current = match self.scenario.dim {
            Some(Dim::D1) => self
                .scenario
                .d1
                .as_ref()
                .map(|g| (&g.counts_current, g.step)),
            Some(Dim::D2) => self
                .scenario
                .d2
                .as_ref()
                .map(|g| (&g.counts_current, g.step)),
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
                while list.len() > window {
                    list.pop_front();
                }
            }
        }

        self.stats.history = history;
        self.stats.show = show;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::state::{
        Chrome, EditState, EditorState, ExportState, Inputs, Playback, Scenario, StatsState,
        ViewSettings,
    };

    /// A real `CellaApp` running the built-in 2D Life demo.
    ///
    /// The production constructor needs an `eframe::CreationContext`, which only
    /// exists once a window is open, so the struct is built field by field here
    /// and then handed the same demo scenario `CellaApp::new` loads.
    fn test_app_with_life() -> CellaApp {
        let ctx = egui::Context::default();
        let mut app = CellaApp {
            scenario: Scenario::default(),
            playback: Playback::default(),
            view: ViewSettings::default(),
            edit: EditState::default(),
            export: ExportState::default(),
            stats: StatsState::default(),
            editor: EditorState::default(),
            chrome: Chrome::new(&ctx),
            inputs: Inputs::default(),
        };
        app.load_demo_life();
        app
    }

    #[test]
    fn run_to_batch_stops_early_when_it_reaches_the_target() {
        let mut app = test_app_with_life();
        let start = app.current_step();

        let done = app.run_to_batch(start + 5, 32);

        assert_eq!(done, 5, "should stop at the target, not run the whole batch");
        assert_eq!(app.current_step(), start + 5);
    }

    #[test]
    fn run_to_batch_stops_at_max_steps_for_a_far_target() {
        let mut app = test_app_with_life();
        let start = app.current_step();

        let done = app.run_to_batch(start + 10_000, 32);

        assert_eq!(done, 32, "a far target should use the whole batch");
        assert_eq!(app.current_step(), start + 32);
    }

    #[test]
    fn run_to_batch_does_nothing_when_the_target_is_already_reached() {
        let mut app = test_app_with_life();
        app.run_to_batch(app.current_step() + 3, 32);
        let start = app.current_step();

        let done = app.run_to_batch(start, 32);

        assert_eq!(done, 0);
        assert_eq!(app.current_step(), start, "the grid must not have stepped");
    }

    #[test]
    fn run_to_batch_advances_the_step_counter_by_what_it_returns() {
        let mut app = test_app_with_life();
        let start = app.current_step();

        let done = app.run_to_batch(start + 7, 32);

        assert_eq!(app.current_step(), start + u64::from(done));
        assert_eq!(done, 7);
    }

    #[test]
    fn a_second_run_to_press_keeps_the_playback_state_from_the_first() {
        let mut app = test_app_with_life();
        let start = app.current_step();
        app.playback.run_to_steps = 3;
        assert!(!app.playback.playing, "the app starts paused");

        // First press: aims 3 steps ahead and forces `playing` on.
        app.start_run_to();
        // Second press while that run is still pending, to extend it. The
        // `playing` it sees is the one the first press forced, not the user's.
        app.playback.run_to_steps = 5;
        app.start_run_to();

        assert_eq!(
            app.playback.run_to_target,
            Some(start + 5),
            "the second press should extend the run"
        );
        assert!(
            !app.playback.playing_before_run_to,
            "the saved state must still be the pause the user was in"
        );

        // Let the run finish.
        for _ in 0..10 {
            app.tick_play();
            if app.playback.run_to_target.is_none() {
                break;
            }
        }

        assert_eq!(app.playback.run_to_target, None, "the run should have finished");
        assert_eq!(app.current_step(), start + 5);
        assert!(
            !app.playback.playing,
            "the user was paused before the first press, so playback must stop"
        );
    }
}
