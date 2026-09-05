//! The two tabbed side panels.
//!
//! The left **control** panel holds what you do to a simulation (load it,
//! edit cells, style the view, watch statistics); the right **workbench**
//! panel holds what you build (the rule, the model's parameters, and the
//! Explore tools). One tab is visible at a time, so a panel never becomes a
//! long scroll of collapsed headers. Both panels collapse from the toolbar or
//! by dragging their edge shut.

use crate::gui::actions::Action;
use crate::gui::app::CellaApp;
use crate::gui::state::{ControlTab, WorkbenchTab};

/// A row of tab labels. Returns the tab the user just picked, if any; the
/// caller turns that into an action rather than writing state mid-draw.
pub(in crate::gui) fn tab_strip<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    current: T,
    tabs: &[(T, &str, &str)],
) -> Option<T> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (tab, label, help) in tabs {
            if ui
                .selectable_label(current == *tab, *label)
                .on_hover_text(*help)
                .clicked()
                && current != *tab
            {
                picked = Some(*tab);
            }
        }
    });
    ui.separator();
    picked
}

const CONTROL_TABS: [(ControlTab, &str, &str); 4] = [
    (
        ControlTab::Scenario,
        "Scenario",
        "Load a demo or a config, resize the grid",
    ),
    (ControlTab::Edit, "Edit", "Paint cells and export a GIF"),
    (
        ControlTab::Style,
        "Style",
        "Theme, colours, grid lines, text size",
    ),
    (
        ControlTab::Stats,
        "Stats",
        "Population counts and the chart",
    ),
];

const WORKBENCH_TABS: [(WorkbenchTab, &str, &str); 3] = [
    (
        WorkbenchTab::Rule,
        "Rule",
        "Declare types and edit the subrule chain",
    ),
    (
        WorkbenchTab::Model,
        "Model",
        "Parameters of the attached model",
    ),
    (WorkbenchTab::Explore, "Explore", "Ensembles and evolution"),
];

impl CellaApp {
    /// The left panel: control tabs.
    pub(in crate::gui) fn ui_control_panel(&mut self, ui: &mut egui::Ui) {
        // Same local-bool dance as the workbench: `show_collapsible` writes
        // back through the `&mut bool` on drag-to-close.
        let mut open = self.chrome.left_open;
        egui::Panel::left("control")
            .resizable(true)
            .min_size(240.0)
            .default_size(300.0)
            .show_collapsible(ui, &mut open, |ui| {
                if let Some(tab) = tab_strip(ui, self.chrome.control_tab, &CONTROL_TABS) {
                    self.push(Action::SetControlTab(tab));
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.chrome.control_tab {
                        ControlTab::Scenario => self.ui_dataset_controls(ui),
                        ControlTab::Edit => self.ui_edit_tab(ui),
                        ControlTab::Style => self.ui_style_tab(ui),
                        ControlTab::Stats => self.ui_statistics(ui),
                    });
            });
        self.chrome.left_open = open;
    }

    /// The right panel: workbench tabs.
    pub(in crate::gui) fn ui_workbench_panel(&mut self, ui: &mut egui::Ui) {
        let mut open = self.chrome.right_open;
        egui::Panel::right("workbench")
            .resizable(true)
            .min_size(340.0)
            .default_size(400.0)
            .max_size(720.0)
            .show_collapsible(ui, &mut open, |ui| {
                if let Some(tab) = tab_strip(ui, self.chrome.workbench_tab, &WORKBENCH_TABS) {
                    self.push(Action::SetWorkbenchTab(tab));
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.chrome.workbench_tab {
                        WorkbenchTab::Rule => self.ui_rule_editor(ui),
                        WorkbenchTab::Model => self.ui_model_tab(ui),
                        WorkbenchTab::Explore => self.ui_explore_tab(ui),
                    });
            });
        self.chrome.right_open = open;
    }

    /// The Model tab: the generic parameter panel, or a note when the loaded
    /// scenario has no model to show.
    fn ui_model_tab(&mut self, ui: &mut egui::Ui) {
        let has_model = self.scenario.d2.as_ref().is_some_and(|g| g.model.is_some());
        if has_model {
            self.ui_model_params(ui);
        } else {
            ui.label("No model attached.");
            ui.small(
                "A 2D config with a \"model\" block (for example configs/2d_wildfire_demo.json) \
                 shows its parameters here.",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::app::DrawMode;
    use crate::gui::sim::tests::test_app;

    #[test]
    fn a_tab_strip_draws_and_picks_nothing_when_untouched() {
        let mut picked = Some(ControlTab::Edit);
        egui::__run_test_ui(|ui| {
            picked = tab_strip(ui, ControlTab::Scenario, &CONTROL_TABS);
        });
        assert_eq!(picked, None);
    }

    #[test]
    fn every_tab_body_draws_headless_for_every_kind_of_scenario() {
        let mut apps = vec![test_app()];
        let mut life = test_app();
        life.load_demo_life();
        apps.push(life);
        let mut rule30 = test_app();
        rule30.load_demo_1d_rule30();
        apps.push(rule30);
        for app in &mut apps {
            for tab in [
                ControlTab::Scenario,
                ControlTab::Edit,
                ControlTab::Style,
                ControlTab::Stats,
            ] {
                app.chrome.control_tab = tab;
                egui::__run_test_ui(|ui| match tab {
                    ControlTab::Scenario => app.ui_dataset_controls(ui),
                    ControlTab::Edit => app.ui_edit_tab(ui),
                    ControlTab::Style => app.ui_style_tab(ui),
                    ControlTab::Stats => app.ui_statistics(ui),
                });
            }
            for tab in [
                WorkbenchTab::Rule,
                WorkbenchTab::Model,
                WorkbenchTab::Explore,
            ] {
                app.chrome.workbench_tab = tab;
                egui::__run_test_ui(|ui| match tab {
                    WorkbenchTab::Rule => app.ui_rule_editor(ui),
                    WorkbenchTab::Model => app.ui_model_tab(ui),
                    WorkbenchTab::Explore => app.ui_explore_tab(ui),
                });
            }
            // The Edit tab has extra rows per tool: brush range + shape, stamp picker.
            for mode in [DrawMode::Paint, DrawMode::Stamp, DrawMode::Cycle] {
                app.edit.draw_mode = mode;
                egui::__run_test_ui(|ui| app.ui_edit_tab(ui));
            }
            assert!(app.actions.is_empty(), "drawing queues nothing");
        }
    }
}
