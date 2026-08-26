//! Driving the simulation forward and recording what happened.
//!
//! This is the clock of the application: it decides *when* a step runs
//! (paced playback, or a "Run to +N" burst), performs the step, and appends
//! the resulting population counts to the statistics series the chart reads.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::app::{CellaApp, Dim};
use cella_lib::types::interner;
use cella_lib::*;
use lasso2::Spur;

/// Upper bound on simulation steps executed in a single frame while a
/// "Run to +N" target is pending, so the UI stays responsive.
pub(in crate::gui) const RUN_TO_STEPS_PER_FRAME: u32 = 100;

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
    /// Play loop with fixed refresh step cadence; also handles "Run to +N".
    pub(in crate::gui) fn tick_play(&mut self) {
        // Fixed refresh: step at most once per refresh interval under play.
        let now = Instant::now();
        let interval = Duration::from_millis(self.playback.refresh_ms);
        if self.playback.playing && now.duration_since(self.playback.last_tick) >= interval {
            self.playback.last_tick = now;
            self.step_once();
        }
        if let Some(target) = self.playback.run_to_target {
            // Cap steps per frame so a large "Run to +N" still lets the UI render.
            for _ in 0..RUN_TO_STEPS_PER_FRAME {
                if self.current_step() >= target {
                    break;
                }
                self.step_once();
            }
            if self.current_step() >= target {
                self.playback.run_to_target = None;
                self.playback.playing = false;
                // Stop timer when run-to completes
                if let Some(start) = self.playback.play_start.take() {
                    self.playback.elapsed += start.elapsed();
                }
            }
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
