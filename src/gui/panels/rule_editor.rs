//! The rule editor panel: declare cell types, then build and apply the
//! subrule chain that drives the automaton.

use super::rule_edit_model::{Rule1DEdit, Rule1DSubruleEdit, Rule2DEdit, Rule2DSubruleEdit};
use crate::gui::app::{CellaApp, Dim};
use crate::gui::panels::widgets::{randomness_control, type_combo};
use cella_lib::types::interner;
use cella_lib::*;

impl CellaApp {
    pub(in crate::gui) fn ui_rule_editor(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Rule editor", |ui| {
            // Manage known types
            ui.label("Types/states available to rules:").on_hover_text("Declare the distinct cell states used by your rules. 'Inactive' is reserved and always present.");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.editor.new_type_name)
                    .on_hover_text("Enter a new state name (e.g., Alive, Dead, A, B). Avoid using 'Inactive'.");
                if ui.button("Add type")
                    .on_hover_text("Add the typed state so it can be used in rules and colored in the viewport.")
                    .clicked() {
                    let name = self.editor.new_type_name.trim();
                    if !name.is_empty() && name != INACTIVE {
                        self.editor.custom_types.insert(interner().get_or_intern(name));
                        self.set_status(format!("Added type '{}'", name));
                        // set a default color if desired (optional; fallback hash works)
                        self.editor.new_type_name.clear();
                    }
                }
            });
            // Show current list (already ordered Inactive-first by `declared_types`)
            let names: Vec<&'static str> = self.declared_types().iter().map(|t| t.as_str()).collect();
            ui.horizontal_wrapped(|ui| {
                for n in &names { ui.label(egui::RichText::new(*n).monospace()); }
            });
            ui.separator();

            if let Some(dim) = self.scenario.dim {
                match dim {
                    Dim::D1 => {
                        // Ensure editor model exists
                        if self.editor.rule_1d.is_none()
                            && let Some(g) = &self.scenario.d1 { self.editor.rule_1d = Some(Rule1DEdit::from_rule(&g.rule)); }
                        if let Some(edit) = &mut self.editor.rule_1d {
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
                                    type_combo(
                                        ui,
                                        ("d1_cur", i),
                                        "current:",
                                        "Center cell must currently be this state for the subrule to apply.",
                                        ty_names,
                                        &mut sub.current,
                                    );
                                    // criteria
                                    type_combo(
                                        ui,
                                        ("d1_crit", i),
                                        "criteria:",
                                        "Neighbor cells equal to this state are treated as 1s in the Wolfram pattern; others are 0s.",
                                        ty_names,
                                        &mut sub.criteria,
                                    );
                                    // output
                                    type_combo(
                                        ui,
                                        ("d1_out", i),
                                        "output:",
                                        "The new state to set when this subrule matches.",
                                        ty_names,
                                        &mut sub.output,
                                    );
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
                                    randomness_control(
                                        ui,
                                        &mut sub.randomness_enabled,
                                        &mut sub.randomness_value,
                                        "Optional stochasticity: when enabled, the match will only apply with probability 1 - p.",
                                    );
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
                                        if let Some(g) = &mut self.scenario.d1 { g.rule = rule; }
                                        self.editor.error_msg = None;
                                        self.refresh_rule_editor_from_current();
                                        self.set_status("Applied 1D rule");
                                    }
                                    Err(e) => { self.editor.error_msg = Some(e.clone()); self.set_status(format!("Rule error: {}", e)); }
                                }
                            }
                        } else {
                            ui.label("No 1D grid loaded.");
                        }
                    }
                    Dim::D2 => {
                        if self.editor.rule_2d.is_none()
                            && let Some(g) = &self.scenario.d2 { self.editor.rule_2d = Some(Rule2DEdit::from_rule(&g.rule)); }
                        if let Some(edit) = &mut self.editor.rule_2d {
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
                                    type_combo(
                                        ui,
                                        ("d2_cur", i),
                                        "current:",
                                        "Center cell must currently be this state for the subrule to apply.",
                                        ty_names,
                                        &mut sub.current,
                                    );
                                    // criteria
                                    type_combo(
                                        ui,
                                        ("d2_crit", i),
                                        "criteria:",
                                        "Neighbor cells of this state are counted within the chosen neighborhood.",
                                        ty_names,
                                        &mut sub.criteria,
                                    );
                                    // output
                                    type_combo(
                                        ui,
                                        ("d2_out", i),
                                        "output:",
                                        "The new state to set when this subrule matches.",
                                        ty_names,
                                        &mut sub.output,
                                    );
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
                                    randomness_control(
                                        ui,
                                        &mut sub.randomness_enabled,
                                        &mut sub.randomness_value,
                                        "Optional stochasticity: when enabled, the match will only apply with probability 1 - p.",
                                    );
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
                                        if let Some(g) = &mut self.scenario.d2 { g.rule = rule; }
                                        self.editor.error_msg = None;
                                        self.refresh_rule_editor_from_current();
                                        self.set_status("Applied 2D rule");
                                    }
                                    Err(e) => { self.editor.error_msg = Some(e.clone()); self.set_status(format!("Rule error: {}", e)); }
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

            if let Some(err) = &self.editor.error_msg { ui.colored_label(egui::Color32::RED, format!("Rule error: {}", err)); }
        });
    }
    /// Build the color editor panel, including the Inactive color.
    pub(in crate::gui) fn refresh_rule_editor_from_current(&mut self) {
        self.editor.error_msg = None;
        match self.scenario.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.scenario.d1 {
                    self.editor.rule_1d = Some(Rule1DEdit::from_rule(&g.rule));
                    self.editor.rule_2d = None;
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.scenario.d2 {
                    self.editor.rule_2d = Some(Rule2DEdit::from_rule(&g.rule));
                    self.editor.rule_1d = None;
                }
            }
            None => {
                self.editor.rule_1d = None;
                self.editor.rule_2d = None;
            }
        }
    }
    /// Show a collapsible panel with per-type statistics (current and peak counts),
    /// and a running history chart (fixed-size window) similar to Task Manager.
    pub(in crate::gui) fn neighborhood_ascii(range: u8, kind: Neighborhood2D) -> String {
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
        let half = if kind == Neighborhood2D::Knight {
            n * 2
        } else {
            n
        };
        for dy in -half..=half {
            for dx in -half..=half {
                if dx == 0 && dy == 0 {
                    out.push('@');
                } else {
                    let inside = neighborhood_contains(dx, dy, n, kind);
                    out.push(if inside { '#' } else { '.' });
                }
            }
            if dy != half {
                out.push('\n');
            }
        }
        out
    }
}
