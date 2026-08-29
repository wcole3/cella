//! Small widget helpers shared by the panels.
//!
//! These exist because the same control kept being written out by hand in
//! several places. Each helper owns one control, so a change to how (say) a
//! cell-type picker looks happens once instead of six times.

/// A labelled cell-type picker.
///
/// `id_salt` must be unique per control on screen — egui uses it to tell two
/// otherwise-identical combo boxes apart. Writes `value` in place and returns
/// `true` when the user picked something different.
pub(in crate::gui) fn type_combo(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    help: &str,
    names: &[&'static str],
    value: &mut String,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(help);
        let mut sel = value.clone();
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(sel.clone())
            .show_ui(ui, |ui| {
                for n in names {
                    ui.selectable_value(&mut sel, (*n).to_owned(), *n);
                }
            });
        if sel != *value {
            *value = sel;
            changed = true;
        }
    });
    changed
}

/// The optional-randomness control: a checkbox that reveals a probability
/// slider only once it is ticked.
pub(in crate::gui) fn randomness_control(
    ui: &mut egui::Ui,
    enabled: &mut bool,
    value: &mut f64,
    help: &str,
) {
    ui.horizontal(|ui| {
        ui.checkbox(enabled, "randomness").on_hover_text(help);
        if *enabled {
            ui.add(
                egui::Slider::new(value, 0.0..=1.0)
                    .text("p")
                    .fixed_decimals(3),
            )
            .on_hover_text(
                "Probability p to cancel the match (so the rule applies with probability 1 - p).",
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a helper through a real (headless) egui pass. `egui::__run_test_ui`
    /// builds a throwaway context so widget code can be exercised without a window.
    #[test]
    fn type_combo_reports_no_change_when_untouched() {
        let mut value = "Alive".to_string();
        let mut changed = None;
        egui::__run_test_ui(|ui| {
            changed = Some(type_combo(
                ui,
                "salt",
                "current:",
                "help",
                &["Inactive", "Alive"],
                &mut value,
            ));
        });
        assert_eq!(changed, Some(false), "no interaction means no change");
        assert_eq!(value, "Alive", "value is left alone");
    }

    #[test]
    fn randomness_control_hides_slider_until_enabled() {
        let mut enabled = false;
        let mut p = 0.25;
        egui::__run_test_ui(|ui| randomness_control(ui, &mut enabled, &mut p, "help"));
        assert!(!enabled);
        assert_eq!(p, 0.25, "disabled control does not touch the value");

        let mut enabled = true;
        egui::__run_test_ui(|ui| randomness_control(ui, &mut enabled, &mut p, "help"));
        assert!(enabled);
        assert_eq!(p, 0.25);
    }
}
