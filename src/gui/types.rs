//! Cell-type helpers shared across the GUI.
//!
//! These answer three questions the panels and the viewport keep asking:
//! which states exist in the current scenario, what order to show them in,
//! and which state a click should cycle to next.

use std::collections::BTreeSet;

use super::app::{CellaApp, Dim};
use cella_lib::*;
use lasso2::Spur;

/// Next type after `current` in `types`, wrapping around. Falls back to `Inactive`
/// when `types` is empty or `current` is not a declared type.
pub(in crate::gui) fn next_in_cycle(types: &[CellType], current: CellType) -> CellType {
    if types.is_empty() {
        return CellType::inactive();
    }
    let idx = types.iter().position(|t| *t == current).unwrap_or(0);
    types[(idx + 1) % types.len()]
}

/// Sort type names so that `Inactive` comes first and the rest are alphabetical.
pub(in crate::gui) fn sort_types_inactive_first(names: &mut [CellType]) {
    names.sort_by(|a, b| {
        let (a, b) = (a.as_str(), b.as_str());
        (a != INACTIVE).cmp(&(b != INACTIVE)).then_with(|| a.cmp(b))
    });
}

impl CellaApp {
    /// Collect all declared types for the current scenario (from rules/config),
    /// including Inactive, regardless of whether they are currently present on the grid.
    ///
    /// Ordered with `Inactive` first, then alphabetically.
    pub(in crate::gui) fn declared_types(&self) -> Vec<CellType> {
        let mut set: BTreeSet<Spur> = BTreeSet::new();
        set.insert(CellType::inactive().0);
        match self.scenario.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.scenario.d1 {
                    set.extend(
                        g.rule
                            .subrules
                            .iter()
                            .flat_map(|s| [s.current_type.0, s.criteria_type.0, s.output_type.0]),
                    );
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.scenario.d2 {
                    set.extend(
                        g.rule
                            .subrules
                            .iter()
                            .flat_map(|s| [s.current_type.0, s.criteria_type.0, s.output_type.0]),
                    );
                    // External models (e.g. wildfire) declare their own palette.
                    if let Some(m) = &g.model {
                        set.extend(m.declared_types().iter().map(|t| t.0));
                    }
                }
            }
            None => {}
        }
        // Include any extra types added via the editor
        set.extend(self.editor.custom_types.iter().copied());

        let mut types: Vec<CellType> = set.into_iter().map(CellType).collect();
        sort_types_inactive_first(&mut types);
        types
    }

    /// Update default draw type to the first non-Inactive type in the grid, else Inactive.
    pub(in crate::gui) fn update_selected_draw_type_default(&mut self) {
        let mut pick: Option<CellType> = None;
        match self.scenario.dim {
            Some(Dim::D1) => {
                if let Some(g) = &self.scenario.d1 {
                    pick = (0..g.width)
                        .map(|i| g.cell_type(i))
                        .find(|t| *t != CellType::inactive());
                }
            }
            Some(Dim::D2) => {
                if let Some(g) = &self.scenario.d2 {
                    pick = (0..g.width * g.height)
                        .map(|i| g.cell_type(i))
                        .find(|t| *t != CellType::inactive());
                }
            }
            None => {}
        }
        self.edit.selected_draw_type = Some(pick.unwrap_or_else(CellType::inactive));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::render::palette_index_for;
    #[test]
    fn next_in_cycle_wraps_and_handles_unknown_types() {
        let types = [
            CellType::from("a"),
            CellType::from("b"),
            CellType::from("c"),
        ];
        assert_eq!(next_in_cycle(&types, types[0]), types[1]);
        assert_eq!(next_in_cycle(&types, types[2]), types[0]);
        // A type that is not declared restarts the cycle at the second entry.
        assert_eq!(next_in_cycle(&types, CellType::from("zzz")), types[1]);
        assert_eq!(next_in_cycle(&[], types[0]), CellType::inactive());
    }

    #[test]
    fn types_sort_with_inactive_first_then_alphabetical() {
        let mut types = vec![
            CellType::from("zeta"),
            CellType::from("alpha"),
            CellType::inactive(),
            CellType::from("beta"),
        ];
        sort_types_inactive_first(&mut types);
        let names: Vec<&str> = types.iter().map(|t| t.as_str()).collect();
        assert_eq!(names, vec![INACTIVE, "alpha", "beta", "zeta"]);
    }

    #[test]
    fn palette_index_is_deterministic_and_in_range() {
        assert_eq!(palette_index_for("Alive", 8), palette_index_for("Alive", 8));
        assert!(palette_index_for("Alive", 8) < 8);
        // Must not divide by zero on an empty palette.
        assert_eq!(palette_index_for("Alive", 0), 0);
    }
}
