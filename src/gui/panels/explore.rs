//! The Explore tab: run the loaded simulation as an ensemble, or evolve its
//! knobs. Every widget reads [`ExploreState`](crate::gui::explore::ExploreState)
//! and pushes an [`Action`] for anything that touches the worker or the grid;
//! plain settings (member counts, ranges) are edited in place, the way the
//! rule editor edits its draft.
//!
//! Nothing here knows which model is loaded: genes come from the grid's own
//! parameter list and tracked types from its declared cell types.

use crate::gui::actions::Action;
use crate::gui::app::CellaApp;
use crate::gui::explore::{
    DescriptorRow, ExploreAction, ExploreMode, GoalChoice, MetricChoice, SearchChoice, can_apply,
    can_start, clamp_range, objective_error, steps_behind_main,
};
use crate::gui::gallery::{
    STRIP_MAX, THUMB_SIDE, cell_tooltip, fitness_t, heat_grid, thumbnail_image, top_elites,
};
use crate::gui::panels::model::value_text;
use crate::gui::render::heat_color;
use crate::gui::theme::{self, SPACE_SM};
use cella_lib::ParamKind;
use egui_plot::{Legend, Line, Plot, PlotPoints};

/// Most members the tab will start; above this the grid clones alone take
/// gigabytes on an ordinary grid.
pub(in crate::gui) const MAX_MEMBERS: usize = 512;

impl CellaApp {
    /// The Explore tab body.
    pub(in crate::gui) fn ui_explore_tab(&mut self, ui: &mut egui::Ui) {
        self.explore.ctx = Some(ui.ctx().clone());
        if self.scenario.dim.is_none() {
            ui.label("Load a simulation first.");
            ui.small("Explore runs many copies of the loaded grid, so it needs one to copy.");
            return;
        }
        self.reconcile_explore_state();

        let mode = self.explore.mode;
        ui.horizontal(|ui| {
            for (m, label, help) in [
                (
                    ExploreMode::MonteCarlo,
                    "Monte Carlo",
                    "Run many copies with different knobs and seeds; see where they agree.",
                ),
                (
                    ExploreMode::Evolve,
                    "Evolve",
                    "Search the knobs for the ones that score best on an objective.",
                ),
            ] {
                if ui
                    .selectable_label(mode == m, label)
                    .on_hover_text(help)
                    .clicked()
                    && mode != m
                {
                    self.push(Action::Explore(ExploreAction::SetMode(m)));
                }
            }
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("explore_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.ui_explore_genes(ui);
                self.ui_explore_tracked(ui);
                match mode {
                    ExploreMode::MonteCarlo => self.ui_explore_monte_carlo(ui),
                    ExploreMode::Evolve => self.ui_explore_evolve(ui),
                }
                if let Some(msg) = &self.explore.message {
                    ui.add_space(SPACE_SM);
                    ui.small(msg.clone());
                }
            });
    }

    /// The genes table: which knobs vary, and over what range.
    fn ui_explore_genes(&mut self, ui: &mut egui::Ui) {
        let mut actions = Vec::new();
        let worker_running = self.explore.worker.is_some();
        theme::section(ui, "Genes", |ui| {
            if self.explore.genes.is_empty() {
                ui.small("This simulation has no adjustable knobs.");
                return;
            }
            ui.horizontal(|ui| {
                ui.small("Tick a knob to let it vary; set the range it may take.");
                if ui.small_button("All").clicked() {
                    actions.push(ExploreAction::VaryAll(true));
                }
                if ui.small_button("None").clicked() {
                    actions.push(ExploreAction::VaryAll(false));
                }
            });
            egui::Grid::new("explore_genes")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    ui.small("vary");
                    ui.small("knob");
                    ui.small("now");
                    ui.small("range");
                    ui.small("log");
                    ui.end_row();
                    for (i, row) in self.explore.genes.iter_mut().enumerate() {
                        let mut vary = row.vary;
                        if ui.checkbox(&mut vary, "").changed() {
                            actions.push(ExploreAction::SetVary(i, vary));
                        }
                        let label = ui.label(&row.desc.key);
                        if let Some(help) = &row.desc.help {
                            label.on_hover_text(help);
                        }
                        ui.monospace(value_text(&row.current));
                        ui.add_enabled_ui(!worker_running, |ui| match &row.desc.kind {
                            ParamKind::Float { .. } | ParamKind::Int { .. } => {
                                let is_int = matches!(row.desc.kind, ParamKind::Int { .. });
                                ui.horizontal(|ui| {
                                    let speed = if is_int {
                                        1.0
                                    } else {
                                        (row.hi - row.lo).abs().max(1e-6) / 100.0
                                    };
                                    let mut lo =
                                        ui.add(egui::DragValue::new(&mut row.lo).speed(speed));
                                    let mut hi =
                                        ui.add(egui::DragValue::new(&mut row.hi).speed(speed));
                                    if is_int {
                                        row.lo = row.lo.round();
                                        row.hi = row.hi.round();
                                    }
                                    if lo.changed() || hi.changed() {
                                        let (a, b) = clamp_range(row.lo, row.hi, &row.desc.kind);
                                        row.lo = a;
                                        row.hi = b;
                                    }
                                    lo = lo.on_hover_text("lowest value the knob may take");
                                    hi = hi.on_hover_text("highest value the knob may take");
                                    let _ = (lo, hi);
                                });
                            }
                            ParamKind::Choice { options } => {
                                ui.horizontal_wrapped(|ui| {
                                    if row.choices.len() != options.len() {
                                        row.choices = vec![true; options.len()];
                                    }
                                    for (o, on) in options.iter().zip(row.choices.iter_mut()) {
                                        ui.checkbox(on, o);
                                    }
                                });
                            }
                            ParamKind::Bool => {
                                ui.small("on / off");
                            }
                            ParamKind::Bits { len } => {
                                ui.small(format!("{len} bits"));
                            }
                        });
                        let numeric = matches!(
                            row.desc.kind,
                            ParamKind::Float { .. } | ParamKind::Int { .. }
                        );
                        ui.add_enabled_ui(numeric && row.lo > 0.0 && !worker_running, |ui| {
                            ui.checkbox(&mut row.log, "").on_hover_text(
                                "sample every decade equally (needs a positive range)",
                            );
                        });
                        ui.end_row();
                    }
                });
            if worker_running {
                ui.small("Ranges are locked while a worker runs; Discard to edit them.");
            }
        });
        for a in actions {
            self.push(Action::Explore(a));
        }
    }

    /// Tracked-type chips: the types the probability map and metrics count.
    fn ui_explore_tracked(&mut self, ui: &mut egui::Ui) {
        let types: Vec<_> = self
            .declared_types()
            .into_iter()
            .filter(|t| *t != cella_lib::CellType::inactive())
            .collect();
        let mut actions = Vec::new();
        theme::section(ui, "Tracked types", |ui| {
            ui.small("Cells in these types are what the probability map and metrics count.");
            ui.horizontal_wrapped(|ui| {
                for t in &types {
                    let on = self.explore.tracked.contains(&t.0);
                    let color = self.color_of(t);
                    let text = egui::RichText::new(t.as_str()).color(if on {
                        egui::Color32::BLACK
                    } else {
                        color
                    });
                    let button = egui::Button::new(text)
                        .fill(if on {
                            color
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::new(1.0, color));
                    if ui.add(button).clicked() {
                        actions.push(ExploreAction::SetTracked(*t, !on));
                    }
                }
            });
        });
        for a in actions {
            self.push(Action::Explore(a));
        }
    }

    /// Monte Carlo controls and readouts.
    fn ui_explore_monte_carlo(&mut self, ui: &mut egui::Ui) {
        let busy = self.explore.worker.as_ref().is_some_and(|w| w.busy);
        let running = self
            .explore
            .worker
            .as_ref()
            .is_some_and(|w| w.mode == ExploreMode::MonteCarlo);
        let mut actions = Vec::new();
        theme::section(ui, "Monte Carlo", |ui| {
            let mc = &mut self.explore.mc;
            ui.add_enabled_ui(!running, |ui| {
                egui::Grid::new("explore_mc").num_columns(2).show(ui, |ui| {
                    ui.label("Members")
                        .on_hover_text("copies of the grid; more = smoother map, more memory");
                    ui.add(egui::Slider::new(&mut mc.members, 1..=MAX_MEMBERS).logarithmic(true));
                    ui.end_row();
                    ui.label("Seed");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut mc.seed));
                        if ui
                            .small_button("↻")
                            .on_hover_text("new random seed")
                            .clicked()
                        {
                            mc.seed = mc
                                .seed
                                .wrapping_mul(6364136223846793005)
                                .wrapping_add(1442695040888963407)
                                >> 8;
                        }
                    });
                    ui.end_row();
                    ui.label("Beta")
                        .on_hover_text("how sharply learning favours good members (10 is gentle)");
                    ui.add(egui::Slider::new(&mut mc.beta, 0.1..=100.0).logarithmic(true));
                    ui.end_row();
                    ui.label("Sigma")
                        .on_hover_text("how far children's knobs move from their parent's");
                    ui.add(egui::Slider::new(&mut mc.sigma, 0.0..=1.0));
                    ui.end_row();
                    ui.label("Immigrants")
                        .on_hover_text("share of fresh random members after each learning step");
                    ui.add(egui::Slider::new(&mut mc.immigrants, 0.0..=1.0));
                    ui.end_row();
                });
            });
            ui.horizontal(|ui| {
                ui.label("Run +");
                ui.add(egui::DragValue::new(&mut mc.steps).range(1..=100_000));
                ui.label("steps");
            });
            let has_grid = self.scenario.dim.is_some();
            ui.horizontal_wrapped(|ui| {
                if !running {
                    if ui
                        .add_enabled(
                            can_start(has_grid, false, 0, ExploreMode::MonteCarlo, true),
                            egui::Button::new("Start"),
                        )
                        .on_hover_text("build the ensemble from the grid as it is now")
                        .clicked()
                    {
                        actions.push(ExploreAction::StartMonteCarlo);
                    }
                } else {
                    let steps = mc.steps;
                    if ui
                        .add_enabled(!busy, egui::Button::new(format!("Run +{steps}")))
                        .clicked()
                    {
                        actions.push(ExploreAction::RunSteps(steps));
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Run to grid step"))
                        .on_hover_text("catch the ensemble up with the main grid's step")
                        .clicked()
                    {
                        actions.push(ExploreAction::RunToMain);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Learn from grid"))
                        .on_hover_text(
                            "score members against the tracked cells on the main grid and resample",
                        )
                        .clicked()
                    {
                        actions.push(ExploreAction::Assimilate);
                    }
                    if ui.add_enabled(busy, egui::Button::new("Stop")).clicked() {
                        actions.push(ExploreAction::Stop);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Discard"))
                        .clicked()
                    {
                        actions.push(ExploreAction::Discard);
                    }
                }
            });
        });
        if running {
            let behind = steps_behind_main(
                self.explore.template_step,
                self.explore.ensemble_steps,
                self.current_step(),
            );
            theme::section(ui, "Ensemble", |ui| {
                ui.label(format!(
                    "{} members at step {}{}",
                    self.explore.mc.members,
                    self.explore.template_step + self.explore.ensemble_steps,
                    if behind > 0 {
                        format!(" ({behind} behind the grid)")
                    } else {
                        String::new()
                    }
                ));
                if busy {
                    ui.add(egui::Spinner::new());
                }
                if let Some(r) = &self.explore.last_report {
                    let mean = r.scores.iter().sum::<f64>() / r.scores.len().max(1) as f64;
                    let best = r.scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    ui.label(format!(
                        "Last learning step: mean IoU {mean:.3}, best {best:.3}, effective members {:.1}, immigrants {}, rejected {}",
                        r.effective_sample_size, r.immigrants, r.rejected
                    ));
                }
                ui.checkbox(&mut self.view.layers.probability, "Show probability layer");
                ui.small("Blue = few members have a tracked cell here; red = most do.");
            });
        }
        for a in actions {
            self.push(Action::Explore(a));
        }
    }

    /// Evolve controls, objective picker, progress and results.
    fn ui_explore_evolve(&mut self, ui: &mut egui::Ui) {
        let busy = self.explore.worker.as_ref().is_some_and(|w| w.busy);
        let running = self
            .explore
            .worker
            .as_ref()
            .is_some_and(|w| w.mode == ExploreMode::Evolve);
        let varying = self.explore.genes.iter().filter(|r| r.vary).count();
        let tracked_any = !self.explore.tracked.is_empty();
        let mut actions = Vec::new();
        theme::section(ui, "Evolve", |ui| {
            let evo = &mut self.explore.evo;
            ui.add_enabled_ui(!running, |ui| {
                egui::Grid::new("explore_evo")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Population");
                        ui.add(egui::Slider::new(&mut evo.population, 2..=256).logarithmic(true));
                        ui.end_row();
                        ui.label("Steps per run")
                            .on_hover_text("how long each candidate is simulated");
                        ui.add(egui::DragValue::new(&mut evo.steps).range(1..=100_000));
                        ui.end_row();
                        ui.label("Repeats")
                            .on_hover_text("seeds averaged per candidate; more = less luck");
                        ui.add(egui::Slider::new(&mut evo.repeats, 1..=10));
                        ui.end_row();
                        ui.label("Seed");
                        ui.add(egui::DragValue::new(&mut evo.seed));
                        ui.end_row();
                        ui.label("Elite").on_hover_text(
                            "best genomes copied unchanged into the next generation",
                        );
                        ui.add(egui::Slider::new(&mut evo.elite, 0..=8));
                        ui.end_row();
                        ui.label("Crossover");
                        ui.add(egui::Slider::new(&mut evo.crossover, 0.0..=1.0));
                        ui.end_row();
                        ui.label("Mutation")
                            .on_hover_text("chance each gene of a child is nudged");
                        ui.add(egui::Slider::new(&mut evo.mutation, 0.0..=1.0));
                        ui.end_row();
                        ui.label("Sigma")
                            .on_hover_text("size of a nudge, as a share of the gene's range");
                        ui.add(egui::Slider::new(&mut evo.sigma, 0.0..=1.0));
                        ui.end_row();
                        ui.label("Immigrants");
                        ui.add(egui::Slider::new(&mut evo.immigrants, 0.0..=1.0));
                        ui.end_row();
                        ui.label("Search");
                        egui::ComboBox::from_id_salt("explore_search")
                            .selected_text(search_label(evo.search))
                            .show_ui(ui, |ui| {
                                for s in [
                                    SearchChoice::Objective,
                                    SearchChoice::Novelty,
                                    SearchChoice::MapElites,
                                ] {
                                    ui.selectable_value(&mut evo.search, s, search_label(s))
                                        .on_hover_text(search_help(s));
                                }
                            });
                        ui.end_row();
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Run +");
                ui.add(egui::DragValue::new(&mut evo.generations).range(1..=10_000));
                ui.label("generations");
            });
        });

        let objective_ok =
            objective_error(&self.explore.objective, self.explore.evo.steps, tracked_any).is_none();
        theme::section(ui, "Objective", |ui| {
            let obj = &mut self.explore.objective;
            ui.add_enabled_ui(!running, |ui| {
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("explore_metric")
                        .selected_text(obj.metric.label())
                        .show_ui(ui, |ui| {
                            for m in MetricChoice::ALL {
                                ui.selectable_value(&mut obj.metric, m, m.label());
                            }
                        });
                    egui::ComboBox::from_id_salt("explore_goal")
                        .selected_text(goal_label(obj.goal))
                        .show_ui(ui, |ui| {
                            for g in [
                                GoalChoice::Maximise,
                                GoalChoice::Minimise,
                                GoalChoice::Target,
                            ] {
                                ui.selectable_value(&mut obj.goal, g, goal_label(g));
                            }
                        });
                    if obj.goal == GoalChoice::Target {
                        ui.add(egui::DragValue::new(&mut obj.target).speed(0.01));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Measured");
                    ui.selectable_value(&mut obj.at_end, true, "at the end");
                    ui.selectable_value(&mut obj.at_end, false, "at step");
                    if !obj.at_end {
                        ui.add(egui::DragValue::new(&mut obj.at_step).range(0..=100_000));
                    }
                });
            });
            if let Some(e) = objective_error(obj, self.explore.evo.steps, tracked_any) {
                ui.colored_label(ui.visuals().warn_fg_color, e);
            }
        });

        if self.explore.evo.search != SearchChoice::Objective {
            theme::section(ui, "Behaviour axes", |ui| {
                ui.small(
                    "What makes two rules different. Each axis is a measurement binned into cells.",
                );
                let evo = &mut self.explore.evo;
                ui.add_enabled_ui(!running, |ui| {
                    let mut remove = None;
                    let n_axes = evo.descriptors.len();
                    for (i, d) in evo.descriptors.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt(("explore_desc", i))
                                .selected_text(d.metric.label())
                                .show_ui(ui, |ui| {
                                    for m in
                                        MetricChoice::ALL.into_iter().filter(|m| m.is_descriptor())
                                    {
                                        ui.selectable_value(&mut d.metric, m, m.label());
                                    }
                                });
                            ui.checkbox(&mut d.mean, "mean over run");
                            ui.add(
                                egui::DragValue::new(&mut d.bins)
                                    .range(2..=32)
                                    .prefix("bins "),
                            );
                            if n_axes > 1 && ui.small_button("\u{2716}").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        evo.descriptors.remove(i);
                    }
                    if evo.descriptors.len() < 3 && ui.small_button("+ axis").clicked() {
                        evo.descriptors.push(DescriptorRow::default());
                    }
                });
            });
        }

        let has_grid = self.scenario.dim.is_some();
        ui.horizontal_wrapped(|ui| {
            if !running {
                if ui
                    .add_enabled(
                        can_start(has_grid, busy, varying, ExploreMode::Evolve, objective_ok),
                        egui::Button::new("Start"),
                    )
                    .on_hover_text(if varying == 0 {
                        "tick at least one gene to vary"
                    } else {
                        "build the population from the grid as it is now"
                    })
                    .clicked()
                {
                    actions.push(ExploreAction::StartEvolve);
                }
            } else {
                let g = self.explore.evo.generations;
                if ui
                    .add_enabled(!busy, egui::Button::new(format!("Run +{g}")))
                    .clicked()
                {
                    actions.push(ExploreAction::RunGenerations(g));
                }
                if ui.add_enabled(busy, egui::Button::new("Stop")).clicked() {
                    actions.push(ExploreAction::Stop);
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Discard"))
                    .clicked()
                {
                    actions.push(ExploreAction::Discard);
                }
            }
            let apply_ok = can_apply(self.explore.best.is_some(), self.playback.playing, busy);
            if ui
                .add_enabled(apply_ok, egui::Button::new("Apply best"))
                .on_hover_text("write the best genome into the main grid (pause first)")
                .clicked()
            {
                actions.push(ExploreAction::ApplyBest);
            }
        });

        if busy && self.explore.gens_requested > 0 {
            let done = self.explore.gens_done.min(self.explore.gens_requested);
            ui.add(
                egui::ProgressBar::new(done as f32 / self.explore.gens_requested as f32).text(
                    format!("generation {done} / {}", self.explore.gens_requested),
                ),
            );
        }

        if !self.explore.fitness.is_empty() {
            theme::section(ui, "Fitness", |ui| {
                let best: PlotPoints = self
                    .explore
                    .fitness
                    .iter()
                    .map(|(g, b, _)| [*g as f64, *b])
                    .collect();
                let mean: PlotPoints = self
                    .explore
                    .fitness
                    .iter()
                    .map(|(g, _, m)| [*g as f64, *m])
                    .collect();
                Plot::new("explore_fitness")
                    .legend(Legend::default())
                    .height(140.0)
                    .allow_drag(false)
                    .allow_zoom(false)
                    .allow_scroll(false)
                    .show(ui, |plot_ui| {
                        plot_ui.line(Line::new("best", best));
                        plot_ui.line(Line::new("mean", mean));
                    });
            });
        }

        if let Some((score, pairs)) = &self.explore.best {
            theme::section(ui, "Best genome", |ui| {
                ui.label(format!("score {score:.4}"));
                egui::Grid::new("explore_best")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for (k, v) in pairs {
                            ui.label(k);
                            ui.monospace(value_text(v));
                            ui.end_row();
                        }
                    });
            });
        }

        self.ui_explore_archive(ui);
        for a in actions {
            self.push(Action::Explore(a));
        }
    }

    /// The MAP-Elites archive: readouts, a heat map of the first two
    /// behaviour axes (click a cell to apply its genome), and a strip of the
    /// fittest elites' thumbnails.
    fn ui_explore_archive(&mut self, ui: &mut egui::Ui) {
        let busy = self.explore.worker.as_ref().is_some_and(|w| w.busy);
        let apply_ok = can_apply(true, self.playback.playing, busy);
        let mut actions = Vec::new();
        // Taken out while drawing so the thumbnail cache (also in `explore`)
        // and the colour lookup on `self` can be used alongside it.
        let Some(owned) = self.explore.archive.take() else {
            return;
        };
        let snap = &owned;
        let cells = snap.cells.len();
        let generation = snap.generation;
        theme::section(ui, "Archive", |ui| {
            ui.label(format!(
                "{} / {cells} cells filled ({:.0} %), QD score {:.2}, best {:.3}, mean {:.3}",
                snap.stats.elites,
                100.0 * snap.stats.coverage,
                snap.stats.qd_score,
                snap.stats.obj_max,
                snap.stats.obj_mean
            ));
            ui.small(if apply_ok {
                "Click a cell or thumbnail to write its genome into the grid."
            } else {
                "Pause (and let the worker finish) to apply a genome."
            });

            // Heat map.
            let grid = heat_grid(snap);
            let width = ui.available_width().max(THUMB_SIDE);
            let cell_side = (width / grid.cols as f32).clamp(4.0, 28.0);
            let size = egui::vec2(cell_side * grid.cols as f32, cell_side * grid.rows as f32);
            let (response, painter) = ui.allocate_painter(size, egui::Sense::click());
            let origin = response.rect.min;
            let empty = ui.visuals().faint_bg_color;
            for row in 0..grid.rows {
                for col in 0..grid.cols {
                    let rect = egui::Rect::from_min_size(
                        origin
                            + egui::vec2(
                                col as f32 * cell_side,
                                (grid.rows - 1 - row) as f32 * cell_side,
                            ),
                        egui::vec2(cell_side, cell_side),
                    )
                    .shrink(0.5);
                    let color = match grid.slot(col, row) {
                        Some((_, f)) => heat_color(fitness_t(snap, f)),
                        None => empty,
                    };
                    painter.rect_filled(rect, 0.0, color);
                }
            }
            let hovered = response.hover_pos().and_then(|p| {
                let col = ((p.x - origin.x) / cell_side).floor();
                let row = grid.rows as f32 - 1.0 - ((p.y - origin.y) / cell_side).floor();
                (col >= 0.0 && row >= 0.0)
                    .then(|| grid.slot(col as usize, row as usize))
                    .flatten()
            });
            if let Some((i, _)) = hovered {
                let tip = cell_tooltip(snap, i, value_text);
                response.clone().on_hover_ui_at_pointer(|ui| {
                    ui.monospace(tip);
                });
                if response.clicked() && apply_ok {
                    actions.push(ExploreAction::ApplyElite(i));
                }
            }
            ui.small(format!(
                "x: {}   y: {}",
                snap.labels.first().map_or("", String::as_str),
                snap.labels.get(1).map_or("", String::as_str)
            ));

            // Thumbnail strip.
            let top = top_elites(snap, STRIP_MAX);
            if !top.is_empty() {
                egui::ScrollArea::horizontal()
                    .id_salt("explore_thumbs")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for i in top {
                                let Some(Some(cell)) = snap.cells.get(i) else {
                                    continue;
                                };
                                let key = (i, generation);
                                if !self.explore.thumbs.contains_key(&key)
                                    && let Some(t) = &cell.thumbnail
                                {
                                    let img = thumbnail_image(t, |c| self.color_of(c));
                                    let tex = ui.ctx().load_texture(
                                        format!("elite_{i}_{generation}"),
                                        img,
                                        egui::TextureOptions::NEAREST,
                                    );
                                    self.explore.thumbs.insert(key, tex);
                                }
                                ui.vertical(|ui| {
                                    let resp = match self.explore.thumbs.get(&key) {
                                        Some(tex) => ui.add(
                                            egui::Image::new((
                                                tex.id(),
                                                egui::vec2(THUMB_SIDE, THUMB_SIDE),
                                            ))
                                            .sense(egui::Sense::click()),
                                        ),
                                        None => ui.add_sized(
                                            [THUMB_SIDE, THUMB_SIDE],
                                            egui::Button::new(format!("#{i}")),
                                        ),
                                    };
                                    let resp =
                                        resp.on_hover_text(cell_tooltip(snap, i, value_text));
                                    if resp.clicked() && apply_ok {
                                        actions.push(ExploreAction::ApplyElite(i));
                                    }
                                    ui.small(format!("{:.3}", cell.fitness));
                                });
                            }
                        });
                    });
            }
        });
        self.explore.archive = Some(owned);
        for a in actions {
            self.push(Action::Explore(a));
        }
    }
}

fn search_label(s: SearchChoice) -> &'static str {
    match s {
        SearchChoice::Objective => "Best score",
        SearchChoice::Novelty => "Novelty",
        SearchChoice::MapElites => "MAP-Elites",
    }
}

fn search_help(s: SearchChoice) -> &'static str {
    match s {
        SearchChoice::Objective => "climb the objective",
        SearchChoice::Novelty => "reward behaviours unlike anything seen so far",
        SearchChoice::MapElites => "keep the best genome for every kind of behaviour",
    }
}

fn goal_label(g: GoalChoice) -> &'static str {
    match g {
        GoalChoice::Maximise => "maximise",
        GoalChoice::Minimise => "minimise",
        GoalChoice::Target => "hit target",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::sim::tests::test_app;

    #[test]
    fn explore_tab_draws_in_every_state() {
        let mut app = test_app();
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        app.load_demo_life();
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        assert!(!app.explore.genes.is_empty(), "genes come from the grid");
        app.explore.mode = ExploreMode::Evolve;
        app.explore.evo.search = SearchChoice::MapElites;
        app.explore.fitness.push((0, 0.5, 0.2));
        app.explore.best = Some((
            0.5,
            vec![(
                "rule.subrules[0].count".into(),
                cella_lib::ParamValue::Int(3),
            )],
        ));
        // A fake archive draws its readouts and offers the best elite.
        use cella_lib::explore::{ArchiveSnapshot, ArchiveStats, SnapshotCell};
        let elite = SnapshotCell {
            fitness: 0.9,
            named: [(
                "rule.subrules[0].count".to_string(),
                cella_lib::ParamValue::Int(6),
            )]
            .into_iter()
            .collect(),
            thumbnail: Some(cella_lib::explore::Thumbnail {
                width: 2,
                height: 2,
                cells: vec![cella_lib::CellType::from("Alive"); 4],
            }),
        };
        app.explore.archive = Some(ArchiveSnapshot {
            dims: vec![2, 2],
            ranges: vec![[0.0, 1.0], [0.0, 1.0]],
            labels: vec!["activity".into(), "entropy".into()],
            cells: vec![
                None,
                Some(SnapshotCell {
                    fitness: 0.1,
                    ..elite.clone()
                }),
                None,
                Some(elite),
            ],
            stats: ArchiveStats {
                elites: 2,
                coverage: 0.5,
                qd_score: 1.0,
                obj_max: 0.9,
                obj_mean: 0.5,
                out_of_range: 0,
            },
            generation: 3,
        });
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        // Drawing built one texture per elite, keyed by cell and generation.
        let mut keys: Vec<_> = app.explore.thumbs.keys().copied().collect();
        keys.sort();
        assert_eq!(keys, vec![(1, 3), (3, 3)]);
        // A colour change and a new snapshot both drop the cache.
        app.apply_action(Action::SetPalette(1));
        assert!(app.explore.thumbs.is_empty());
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        assert_eq!(app.explore.thumbs.len(), 2);
        let (tx, _rx_cmd) = std::sync::mpsc::channel();
        let (_tx_msg, rx) = std::sync::mpsc::channel();
        app.explore.worker = Some(crate::gui::explore::ExploreWorker {
            tx,
            rx,
            join: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            mode: ExploreMode::Evolve,
            busy: false,
            sig: (crate::gui::app::Dim::D2, 50, 30),
        });
        let mut again = app.explore.archive.clone().unwrap();
        again.generation = 4;
        crate::gui::explore::handle_worker_msg(
            &mut app.explore,
            &mut app.view.layers,
            crate::gui::explore::WorkerMsg::Archive(again),
        );
        assert!(app.explore.thumbs.is_empty());
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        assert!(app.explore.thumbs.contains_key(&(3, 4)));
        app.explore.worker = None;
        // Applying the best elite (cell 3) lands its genome; an empty cell is refused.
        app.apply_action(Action::Explore(ExploreAction::ApplyElite(3)));
        assert_eq!(app.scenario.d2.as_ref().unwrap().rule.subrules[0].count, 6);
        app.apply_action(Action::Explore(ExploreAction::ApplyElite(0)));
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("empty")
        );
        app.load_demo_1d_rule30();
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        assert!(
            app.explore
                .genes
                .iter()
                .any(|r| r.desc.key.ends_with("wolfram_code"))
        );
        for s in [
            SearchChoice::Objective,
            SearchChoice::Novelty,
            SearchChoice::MapElites,
        ] {
            assert!(!search_label(s).is_empty() && !search_help(s).is_empty());
        }
        for g in [
            GoalChoice::Maximise,
            GoalChoice::Minimise,
            GoalChoice::Target,
        ] {
            assert!(!goal_label(g).is_empty());
        }
    }

    #[test]
    fn explore_tab_drives_a_real_worker_through_actions() {
        let mut app = test_app();
        app.load_demo_life();
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        app.explore.mc.members = 2;
        app.apply_action(Action::Explore(ExploreAction::SetVary(0, true)));
        assert!(app.explore.genes[0].vary);
        app.apply_action(Action::Explore(ExploreAction::StartMonteCarlo));
        assert!(app.explore.worker.is_some());
        assert!(app.view.layers.probability);
        // Wait for the construction message, then draw the running tab.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.explore.worker.as_ref().is_some_and(|w| w.busy)
            && std::time::Instant::now() < deadline
        {
            app.poll_explore();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            app.view.layers.probability_map.is_some(),
            "the first map arrived"
        );
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        app.apply_action(Action::Explore(ExploreAction::RunSteps(2)));
        assert!(app.explore.worker.as_ref().unwrap().busy);
        app.apply_action(Action::Explore(ExploreAction::RunSteps(2)));
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("busy")
        );
        while app.explore.worker.as_ref().is_some_and(|w| w.busy)
            && std::time::Instant::now() < deadline
        {
            app.poll_explore();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.explore.ensemble_steps, 2);
        app.apply_action(Action::Explore(ExploreAction::RunToMain));
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("level")
        );
        app.apply_action(Action::Explore(ExploreAction::SetTracked(
            cella_lib::CellType::from("Alive"),
            false,
        )));
        assert!(app.explore.tracked.is_empty());
        app.apply_action(Action::Explore(ExploreAction::Discard));
        assert!(app.explore.worker.is_none() && app.view.layers.probability_map.is_none());
        // Without a worker, run commands only leave a status.
        app.apply_action(Action::Explore(ExploreAction::RunSteps(1)));
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("Start")
        );
        // Apply best while playing is refused; paused it lands.
        app.explore.best = Some((
            1.0,
            vec![(
                "rule.subrules[0].count".into(),
                cella_lib::ParamValue::Int(7),
            )],
        ));
        app.playback.playing = true;
        app.apply_action(Action::Explore(ExploreAction::ApplyBest));
        assert_ne!(app.scenario.d2.as_ref().unwrap().rule.subrules[0].count, 7);
        app.playback.playing = false;
        app.apply_action(Action::Explore(ExploreAction::ApplyBest));
        assert_eq!(app.scenario.d2.as_ref().unwrap().rule.subrules[0].count, 7);
        app.apply_action(Action::Explore(ExploreAction::ApplyElite(0)));
        assert!(
            app.chrome
                .status_message
                .as_deref()
                .unwrap()
                .contains("empty")
        );
        // Evolve refuses to start with nothing varying, then starts.
        app.apply_action(Action::Explore(ExploreAction::SetMode(ExploreMode::Evolve)));
        app.apply_action(Action::Explore(ExploreAction::VaryAll(false)));
        app.explore.objective.metric = MetricChoice::Fraction;
        app.explore.tracked.clear();
        app.apply_action(Action::Explore(ExploreAction::StartEvolve));
        assert!(app.explore.worker.is_none());
        app.reconcile_explore_state();
        app.apply_action(Action::Explore(ExploreAction::SetVary(0, true)));
        app.explore.evo.population = 3;
        app.explore.evo.steps = 3;
        app.explore.evo.repeats = 1;
        app.apply_action(Action::Explore(ExploreAction::StartEvolve));
        assert!(app.explore.worker.is_some());
        while app.explore.worker.as_ref().is_some_and(|w| w.busy)
            && std::time::Instant::now() < deadline
        {
            app.poll_explore();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.apply_action(Action::Explore(ExploreAction::RunGenerations(1)));
        while app.explore.worker.as_ref().is_some_and(|w| w.busy)
            && std::time::Instant::now() < deadline
        {
            app.poll_explore();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.explore.fitness.len(), 1);
        egui::__run_test_ui(|ui| app.ui_explore_tab(ui));
        app.apply_action(Action::Explore(ExploreAction::Stop));
        app.apply_action(Action::Explore(ExploreAction::Discard));
    }
}
