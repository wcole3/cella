//! The model parameter panel: one control per parameter an attached
//! [`cella_lib::ExternalModel`] says it has.
//!
//! The panel is *generic*. It asks the model what its parameters are
//! (`params()`), what they are worth right now (`get_param`), and writes edits
//! back through `Grid2D::set_model_param`. That is the one rule this file
//! lives by: **no model-specific code**. Nothing here may name a particular
//! model, its parameter keys, or its groups — a new model gets a panel for
//! free, and an old one cannot break this file by changing its parameters.

use std::collections::HashMap;

use crate::gui::app::CellaApp;
use cella_lib::{GridState, ParamDesc, ParamKind, ParamValue};

impl CellaApp {
    /// Draw one control per parameter the attached model declares.
    ///
    /// Renders nothing at all when no model is attached, so a Life or Rule 30
    /// scenario does not grow an empty section. Everything worth testing lives
    /// in [`group_params`], [`commit_on`], [`gesture_ended`] and
    /// [`CellaApp::apply_model_param`]; what is left here is the egui plumbing.
    pub(in crate::gui) fn ui_model_params(&mut self, ui: &mut egui::Ui) {
        let Some(model) = self.scenario.d2.as_ref().and_then(|g| g.model.as_ref()) else {
            return;
        };
        // Read everything the panel needs while the model is borrowed, so the
        // widgets below are free to borrow `self` mutably and commit an edit.
        let title = model.typetag_name().to_string();
        let descs = model.params();
        let values: HashMap<String, ParamValue> = descs
            .iter()
            .filter_map(|d| Some((d.key.clone(), model.get_param(&d.key)?)))
            .collect();
        let groups = group_params(descs);

        // One slot is enough: a person finishes at most one control per frame,
        // and the edit has to leave the closures before `self` can be touched.
        let mut commit: Option<(String, ParamValue)> = None;
        // Open on first sight: a section that starts closed hides the whole
        // feature behind a click nobody knows to make.
        egui::CollapsingHeader::new(title)
            .default_open(true)
            .show(ui, |ui| {
                for (group, descs) in &groups {
                    if let Some(name) = group {
                        ui.separator();
                        ui.strong(name);
                    }
                    for desc in descs {
                        // A key `params` lists but `get_param` will not answer
                        // has no current value to draw, so it is skipped.
                        let Some(value) = values.get(&desc.key) else {
                            continue;
                        };
                        if let Some(edited) = param_control(ui, desc, value) {
                            commit = Some((desc.key.clone(), edited));
                        }
                    }
                }
            });
        // The rule the left panel follows is one separator between sections.
        // It lives here rather than at the call site because this panel is the
        // only one that can draw nothing at all: put it there and a scenario
        // with no model would show two separators with nothing between them.
        ui.separator();
        if let Some((key, value)) = commit {
            self.apply_model_param(&key, value);
        }
    }

    /// Write one edited parameter to the live grid, and — only when the grid
    /// accepts it — to the snapshot Reset restores as well.
    ///
    /// A refusal goes to the status bar and nothing else: `set_model_param`
    /// has already rolled the model back, and the control redraws from
    /// `get_param` on the next frame, so there is no local state to undo.
    ///
    /// Does nothing when no 2D grid is loaded. The panel only calls this once
    /// it has drawn a control, so that cannot happen from there; the guard is
    /// here so a future caller cannot turn a missing grid into a panic.
    pub(in crate::gui) fn apply_model_param(&mut self, key: &str, value: ParamValue) {
        let Some(grid) = self.scenario.d2.as_mut() else {
            return;
        };
        match grid.set_model_param(key, value.clone()) {
            Ok(()) => self.mirror_param_into_initial_state(key, value),
            Err(e) => self.set_status(format!("Model parameter error: {e}")),
        }
    }

    /// Apply an accepted parameter to the model inside the Reset snapshot.
    ///
    /// `Scenario::initial_state` holds a *clone* of the model taken when the
    /// scenario was loaded, with the parameters it had then. Left alone, Reset
    /// would put those old parameters back and a slider someone had just moved
    /// would snap — the wrong surprise while tuning a model. Copying the
    /// accepted value across means Reset rewinds the cells, not the sliders.
    ///
    /// A plain `set_param` is the right call here: this model is not attached
    /// to a grid, and `Grid2D::from_state` re-runs `attach` when Reset restores
    /// it. The value was already accepted by the live model, so a failure here
    /// would mean the two copies disagree about their own parameters — nothing
    /// anyone could act on, so it is ignored.
    pub(in crate::gui) fn mirror_param_into_initial_state(&mut self, key: &str, value: ParamValue) {
        if let Some(GridState::D2 {
            model: Some(snapshot),
            ..
        }) = self.scenario.initial_state.as_mut()
        {
            let _ = snapshot.set_param(key, value);
        }
    }
}

/// Split parameters into the sections the panel draws, in a stable order.
///
/// The ungrouped bucket comes first however late the model lists it, then each
/// named group in the order it first appears; inside a bucket the model's own
/// order is kept. Order has to be stable because controls that reshuffle
/// between frames are impossible to use.
fn group_params(descs: Vec<ParamDesc>) -> Vec<(Option<String>, Vec<ParamDesc>)> {
    let mut out: Vec<(Option<String>, Vec<ParamDesc>)> = Vec::new();
    for desc in descs {
        match out.iter_mut().find(|(group, _)| *group == desc.group) {
            Some((_, bucket)) => bucket.push(desc),
            None => out.push((desc.group.clone(), vec![desc])),
        }
    }
    if let Some(i) = out.iter().position(|(group, _)| group.is_none()) {
        let ungrouped = out.remove(i);
        out.insert(0, ungrouped);
    }
    out
}

/// Whether this frame is where an edit to `desc` should be written.
///
/// A cheap parameter commits the moment its widget changes, so dragging its
/// slider looks live. An expensive one — `reattach`, meaning the model rebuilds
/// derived state on every write — waits for the end of the gesture (see
/// [`gesture_ended`]), so the rebuild happens once per drag instead of once per
/// frame. A read-only parameter never commits at all.
fn commit_on(desc: &ParamDesc, changed: bool, ended: bool) -> bool {
    if desc.read_only {
        return false;
    }
    if desc.reattach { ended } else { changed }
}

/// Whether the person is *done* with this control for now.
///
/// Only expensive (`reattach`) parameters ask, because they pay a model
/// rebuild per commit and so should not commit mid-drag. Three things end a
/// gesture: letting go of a drag, moving focus off the control, and — the case
/// that is easy to miss — any change that was not made by dragging at all. A
/// focused slider accepts arrow keys, and a keystroke is a complete gesture the
/// moment it lands; waiting for a drag that never starts would throw the edit
/// away and redraw the old value on the next frame.
fn gesture_ended(changed: bool, dragged: bool, drag_stopped: bool, lost_focus: bool) -> bool {
    drag_stopped || lost_focus || (changed && !dragged)
}

/// Whether this frame commits `desc`, given the widget that just drew it.
///
/// `edited` is the only thing that makes a commit possible: it says the
/// widget's value really differs from the one the model holds. Without that
/// guard a bare click, a Tab away, or a widget that quietly reformatted its own
/// value would write the model a value it already had — and, for a `reattach`
/// parameter, pay a full rebuild for it. Everything else is timing, and that is
/// [`commit_on`]'s decision.
fn commit_now(desc: &ParamDesc, resp: &egui::Response, edited: bool) -> bool {
    let ended = gesture_ended(
        // A combo box reports no change of its own, so `edited` stands in for
        // it: the selection landed in a variable, not in the response.
        resp.changed() || edited,
        resp.dragged(),
        resp.drag_stopped(),
        resp.lost_focus(),
    );
    edited && commit_on(desc, edited, ended)
}

/// Draw the control for one parameter, and return the value to write if this
/// frame is where the edit should be committed.
///
/// The `ParamKind` picks the widget; the current `ParamValue` supplies what it
/// starts at. A value whose shape does not match the kind the descriptor
/// promised is not drawable, so nothing is drawn for it.
fn param_control(ui: &mut egui::Ui, desc: &ParamDesc, value: &ParamValue) -> Option<ParamValue> {
    if desc.read_only {
        with_help(
            ui.label(format!("{}: {}", desc.label, value_text(value))),
            desc,
        );
        return None;
    }
    match (&desc.kind, value) {
        (ParamKind::Float { min, max, step }, ParamValue::Float(current)) => {
            let mut v = *current;
            let mut slider = egui::Slider::new(&mut v, *min..=*max)
                .step_by(*step)
                // Clamp what someone *types*, never what the model already
                // holds: the default rounds the existing value onto the step
                // grid on every draw and reports that as an edit, which would
                // rewrite a value the model is perfectly happy with the first
                // time the panel is drawn.
                .clamping(egui::SliderClamping::Edits)
                .text(&desc.label);
            if let Some(unit) = &desc.unit {
                slider = slider.suffix(unit);
            }
            let resp = with_help(ui.add(slider), desc);
            commit_now(desc, &resp, v != *current).then_some(ParamValue::Float(v))
        }
        (ParamKind::Int { min, max }, ParamValue::Int(current)) => {
            let mut v = *current;
            let resp = ui
                .horizontal(|ui| {
                    let resp = ui.add(
                        egui::DragValue::new(&mut v)
                            .range(*min..=*max)
                            // Same reason as the slider above: an existing
                            // value outside the range is shown as it is, not
                            // silently pulled to the nearest end stop.
                            .clamp_existing_to_range(false),
                    );
                    ui.label(&desc.label);
                    resp
                })
                .inner;
            let resp = with_help(resp, desc);
            commit_now(desc, &resp, v != *current).then_some(ParamValue::Int(v))
        }
        (ParamKind::Bool, ParamValue::Bool(current)) => {
            let mut v = *current;
            let resp = with_help(ui.checkbox(&mut v, &desc.label), desc);
            commit_now(desc, &resp, v != *current).then_some(ParamValue::Bool(v))
        }
        (ParamKind::Choice { options }, ParamValue::Choice(current)) => {
            let mut sel = current.clone();
            // The response carries the tooltip and the focus signals; what it
            // does not carry is the selection, which lands in `sel` below.
            let resp = with_help(
                egui::ComboBox::from_label(&desc.label)
                    .selected_text(sel.clone())
                    .show_ui(ui, |ui| {
                        for opt in options {
                            ui.selectable_value(&mut sel, opt.clone(), opt);
                        }
                    })
                    .response,
                desc,
            );
            // A dropdown has no `changed` of its own: the selection landed in
            // `sel`, so comparing it with the current value is the change.
            commit_now(desc, &resp, sel != *current).then_some(ParamValue::Choice(sel))
        }
        _ => None,
    }
}

/// Attach the descriptor's tooltip to a widget, if it has one.
fn with_help(resp: egui::Response, desc: &ParamDesc) -> egui::Response {
    match &desc.help {
        Some(help) => resp.on_hover_text(help),
        None => resp,
    }
}

/// A parameter value written out for a read-only row, without the `Debug`
/// wrapper a reader would have to look past.
fn value_text(value: &ParamValue) -> String {
    match value {
        ParamValue::Float(v) => v.to_string(),
        ParamValue::Int(v) => v.to_string(),
        ParamValue::Bool(v) => v.to_string(),
        ParamValue::Choice(v) => v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One parameter descriptor, with only the fields a grouping or commit
    /// decision looks at spelled out.
    fn desc(key: &str, group: Option<&str>, reattach: bool, read_only: bool) -> ParamDesc {
        ParamDesc {
            key: key.to_string(),
            label: key.to_string(),
            group: group.map(str::to_string),
            help: None,
            unit: None,
            kind: ParamKind::Bool,
            reattach,
            read_only,
        }
    }

    /// The keys each bucket holds, which is all these tests care about.
    fn shape(groups: &[(Option<String>, Vec<ParamDesc>)]) -> Vec<(Option<String>, Vec<String>)> {
        groups
            .iter()
            .map(|(g, ds)| (g.clone(), ds.iter().map(|d| d.key.clone()).collect()))
            .collect()
    }

    #[test]
    fn group_params_returns_no_buckets_for_no_parameters() {
        assert!(group_params(Vec::new()).is_empty());
    }

    #[test]
    fn group_params_puts_every_ungrouped_parameter_in_one_bucket_in_order() {
        let out = group_params(vec![
            desc("a", None, false, false),
            desc("b", None, false, false),
        ]);

        assert_eq!(
            shape(&out),
            vec![(None, vec!["a".to_string(), "b".to_string()])]
        );
    }

    #[test]
    fn group_params_lists_the_ungrouped_bucket_first_even_when_it_appears_last() {
        let out = group_params(vec![
            desc("a", Some("Wind"), false, false),
            desc("b", None, false, false),
        ]);

        assert_eq!(
            shape(&out),
            vec![
                (None, vec!["b".to_string()]),
                (Some("Wind".to_string()), vec!["a".to_string()]),
            ]
        );
    }

    #[test]
    fn group_params_keeps_interleaved_groups_in_first_appearance_order() {
        let out = group_params(vec![
            desc("a1", Some("A"), false, false),
            desc("b1", Some("B"), false, false),
            desc("a2", Some("A"), false, false),
            desc("b2", Some("B"), false, false),
        ]);

        assert_eq!(
            shape(&out),
            vec![
                (
                    Some("A".to_string()),
                    vec!["a1".to_string(), "a2".to_string()]
                ),
                (
                    Some("B".to_string()),
                    vec!["b1".to_string(), "b2".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn a_cheap_parameter_commits_as_soon_as_the_widget_changes() {
        let cheap = desc("a", None, false, false);

        assert!(commit_on(&cheap, true, false), "changed alone is enough");
        assert!(commit_on(&cheap, true, true));
        assert!(!commit_on(&cheap, false, true), "no change, nothing to do");
        assert!(!commit_on(&cheap, false, false));
    }

    #[test]
    fn an_expensive_parameter_waits_until_the_gesture_ends() {
        let expensive = desc("a", None, true, false);

        assert!(
            !commit_on(&expensive, true, false),
            "mid-drag changes are skipped so the rebuild runs once"
        );
        assert!(commit_on(&expensive, true, true));
        assert!(commit_on(&expensive, false, true), "release always commits");
        assert!(!commit_on(&expensive, false, false));
    }

    /// The gesture-end rule, spelled out for every combination that matters.
    ///
    /// The case that used to be wrong is the first one: a focused slider takes
    /// arrow keys, which change its value with no pointer drag anywhere. An
    /// expensive parameter that waited for `drag_stopped` threw that keystroke
    /// away, redrew from the model next frame, and then committed the *old*
    /// value when focus moved on — a rebuild that changed nothing.
    #[test]
    fn any_change_outside_a_drag_ends_the_gesture_by_itself() {
        // changed, dragged, drag_stopped, lost_focus
        assert!(
            gesture_ended(true, false, false, false),
            "an arrow-key edit is a whole gesture on its own"
        );
        assert!(
            !gesture_ended(true, true, false, false),
            "a change under the pointer is mid-drag, so it waits"
        );
        assert!(
            gesture_ended(true, true, true, false),
            "the frame the drag ends on commits it"
        );
        assert!(
            gesture_ended(false, false, true, false),
            "releasing the slider ends the gesture"
        );
        assert!(
            gesture_ended(false, false, false, true),
            "so does clicking away from it"
        );
        assert!(
            !gesture_ended(false, false, false, false),
            "an untouched frame is not the end of anything"
        );
    }

    #[test]
    fn read_only_values_read_as_plain_text() {
        assert_eq!(value_text(&ParamValue::Float(7.5)), "7.5");
        assert_eq!(value_text(&ParamValue::Int(42)), "42");
        assert_eq!(value_text(&ParamValue::Bool(true)), "true");
        assert_eq!(value_text(&ParamValue::Choice("Moore".into())), "Moore");
    }

    /// Every kind of control the panel knows how to draw, plus a read-only row
    /// and a value whose shape contradicts its descriptor.
    ///
    /// `egui::__run_test_ui` builds a throwaway context, so the widget code
    /// really runs — no window, no GPU. Nobody touches anything during the
    /// pass, so every control must report that there is nothing to commit.
    #[test]
    fn an_untouched_control_of_any_kind_commits_nothing() {
        let float = ParamDesc {
            kind: ParamKind::Float {
                min: 0.0,
                max: 10.0,
                step: 0.5,
            },
            unit: Some("m/s".into()),
            help: Some("a tooltip".into()),
            ..desc("float", Some("Group"), false, false)
        };
        let int = ParamDesc {
            kind: ParamKind::Int { min: 0, max: 10 },
            ..desc("int", None, true, false)
        };
        let boolean = desc("bool", None, false, false);
        let choice = ParamDesc {
            kind: ParamKind::Choice {
                options: vec!["one".into(), "two".into()],
            },
            ..desc("choice", None, false, false)
        };
        let locked = ParamDesc {
            kind: ParamKind::Int { min: 0, max: 10 },
            ..desc("locked", None, false, true)
        };
        // A descriptor that promises a number next to a value that is a name:
        // not drawable, so the panel draws nothing for it.
        let mismatched = (&float, ParamValue::Choice("nonsense".into()));

        let rows = [
            (&float, ParamValue::Float(1.0)),
            (&int, ParamValue::Int(3)),
            (&boolean, ParamValue::Bool(true)),
            (&choice, ParamValue::Choice("one".into())),
            (&locked, ParamValue::Int(7)),
            mismatched,
        ];
        let mut committed = Vec::new();
        egui::__run_test_ui(|ui| {
            committed = rows
                .iter()
                .map(|(desc, value)| param_control(ui, desc, value))
                .collect();
        });

        assert_eq!(committed, vec![None; rows.len()]);
    }

    /// A value that is not on its descriptor's step grid, or that sits outside
    /// the range the descriptor declares, must be *shown* — never quietly
    /// rewritten by the widget that displays it.
    ///
    /// egui's default clamping rounds an existing value onto the slider's step
    /// grid on every draw and reports the widget as changed, so merely opening
    /// the panel would commit `c2 = 0.131` as `0.13` — and mirror the rounded
    /// value into the snapshot Reset restores. Nobody touched anything here,
    /// so there is nothing to commit.
    #[test]
    fn a_value_off_the_step_grid_or_outside_the_range_is_shown_not_rewritten() {
        let off_grid = ParamDesc {
            kind: ParamKind::Float {
                min: 0.0,
                max: 1.0,
                step: 0.005,
            },
            ..desc("off_grid", None, false, false)
        };
        let out_of_range = ParamDesc {
            kind: ParamKind::Int { min: 1, max: 1000 },
            ..desc("out_of_range", None, false, false)
        };

        let mut committed = Vec::new();
        egui::__run_test_ui(|ui| {
            committed = vec![
                param_control(ui, &off_grid, &ParamValue::Float(0.131)),
                param_control(ui, &out_of_range, &ParamValue::Int(1500)),
            ];
        });

        assert_eq!(
            committed,
            vec![None, None],
            "an untouched control must not commit a value the widget invented"
        );
    }

    #[test]
    fn a_read_only_parameter_never_commits() {
        for reattach in [false, true] {
            let locked = desc("a", None, reattach, true);
            for changed in [false, true] {
                for drag_stopped in [false, true] {
                    assert!(
                        !commit_on(&locked, changed, drag_stopped),
                        "read-only parameters are never written"
                    );
                }
            }
        }
    }
}
