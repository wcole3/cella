//! The statistics panel: a current/peak table plus a rolling population chart.

use crate::gui::app::{CellaApp, Dim};
use crate::gui::types::sort_types_inactive_first;
use cella_lib::*;
use egui_plot::{Legend, Line, Plot, PlotPoints};
use lasso2::Spur;
use std::collections::BTreeSet;

impl CellaApp {
    pub(in crate::gui) fn ui_statistics(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Statistics", |ui| {
            // Current/peak table
            let mut entries: Vec<(CellType, u64, u64)> = Vec::new();
            {
                let counts_and_peaks = match self.scenario.dim {
                    Some(Dim::D1) => self
                        .scenario
                        .d1
                        .as_ref()
                        .map(|g| (&g.counts_current, &g.peak_counts)),
                    Some(Dim::D2) => self
                        .scenario
                        .d2
                        .as_ref()
                        .map(|g| (&g.counts_current, &g.peak_counts)),
                    None => None,
                };
                if let Some((counts, peaks)) = counts_and_peaks {
                    let keys: BTreeSet<Spur> = counts.keys().chain(peaks.keys()).copied().collect();
                    entries.extend(keys.into_iter().map(|k| {
                        (
                            CellType(k),
                            counts.get(&k).copied().unwrap_or(0),
                            peaks.get(&k).copied().unwrap_or(0),
                        )
                    }));
                }
            }
            entries.sort_by(|a, b| {
                let (a, b) = (a.0.as_str(), b.0.as_str());
                (a != INACTIVE).cmp(&(b != INACTIVE)).then_with(|| a.cmp(b))
            });
            let total = entries
                .iter()
                .fold(0u64, |acc, (_, c, _)| acc.saturating_add(*c));
            ui.label(format!("Total cells: {}", total));
            for (ty, cur, peak) in &entries {
                ui.label(format!("{:>10}: {} (peak {})", ty.as_str(), cur, peak));
            }
            ui.separator();

            // Visibility toggles
            ui.label("Series shown in graph:");
            let mut series: Vec<CellType> =
                self.stats.history.keys().copied().map(CellType).collect();
            sort_types_inactive_first(&mut series);
            ui.horizontal_wrapped(|ui| {
                for ty in &series {
                    let mut show = self.stats.show.get(&ty.0).copied().unwrap_or(false);
                    let label = egui::RichText::new(ty.as_str()).color(self.color_of(ty));
                    if ui.checkbox(&mut show, label).changed() {
                        self.stats.show.insert(ty.0, show);
                    }
                }
            });

            // Line plot of the last N samples per selected series
            let plot = Plot::new("stats_plot").legend(Legend::default());
            plot.show(ui, |plot_ui| {
                for ty in &series {
                    if !self.stats.show.get(&ty.0).copied().unwrap_or(false) {
                        continue;
                    }
                    let Some(list) = self.stats.history.get(&ty.0) else {
                        continue;
                    };
                    if list.is_empty() {
                        continue;
                    }
                    let pts: PlotPoints = list
                        .iter()
                        .map(|(s, v)| [*s as f64, *v as f64])
                        .collect::<Vec<_>>()
                        .into();
                    plot_ui.line(Line::new(ty.as_str(), pts).color(self.color_of(ty)));
                }
            });
        });
    }
}
