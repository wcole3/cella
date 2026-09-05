//! Grid state snapshots and (de)serialization helpers.

use crate::CellType;
use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;
use crate::rules::{Rule1D, Rule2D};
use crate::types::{CellState, interner};
use lasso2::Spur;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Serializable snapshot of either a 1D or 2D grid.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GridState {
    D1 {
        width: usize,
        history_limit: usize,
        cell_states: Vec<CellState>,
        step: u64,
        #[serde(default)]
        seed: u64,
        rule: Rule1D,
        #[serde(default)]
        counts_current: HashMap<String, u64>,
        #[serde(default)]
        peak_counts: HashMap<String, u64>,
    },
    D2 {
        width: usize,
        height: usize,
        history_limit: usize,
        cell_states: Vec<CellState>,
        step: u64,
        #[serde(default)]
        seed: u64,
        rule: Rule2D,
        #[serde(default)]
        counts_current: HashMap<String, u64>,
        #[serde(default)]
        peak_counts: HashMap<String, u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<Box<dyn crate::external::ExternalModel>>,
    },
}

impl GridState {
    pub fn from_grid1d(g: &Grid1D) -> Self {
        let (current_count_map, peak_count_map) =
            convert_map_spur_to_string(&g.counts_current, &g.peak_counts);
        let cell_states = g.to_cell_states();
        Self::D1 {
            width: g.width,
            history_limit: g.history_limit,
            cell_states,
            step: g.step,
            seed: g.seed,
            rule: g.rule.clone(),
            counts_current: current_count_map,
            peak_counts: peak_count_map,
        }
    }

    pub fn from_grid2d(g: &Grid2D) -> Self {
        let (current_count_map, peak_count_map) =
            convert_map_spur_to_string(&g.counts_current, &g.peak_counts);
        let cell_states = g.to_cell_states();
        Self::D2 {
            width: g.width,
            height: g.height,
            history_limit: g.history_limit,
            cell_states,
            step: g.step,
            seed: g.seed,
            rule: g.rule.clone(),
            counts_current: current_count_map,
            peak_counts: peak_count_map,
            model: g.model.clone(),
        }
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

impl Grid1D {
    pub fn from_state(state: &GridState) -> Option<Self> {
        match state {
            GridState::D1 {
                width,
                history_limit,
                cell_states,
                step,
                seed,
                rule,
                counts_current,
                peak_counts,
            } => {
                let (new_counts, new_peak_counts) =
                    convert_map_string_to_spur(cell_states, counts_current, peak_counts);
                let cells: Vec<CellType> = cell_states.iter().map(|c| c.current).collect();
                let next_cells: Vec<CellType> = vec![CellType::inactive(); *width];
                let ages: Vec<u32> = cell_states.iter().map(|c| c.age_in_state).collect();
                let history_data = soa_history(cell_states, *history_limit);
                let history_heads = soa_heads(cell_states, *history_limit);
                let history_counts = soa_counts(cell_states, *history_limit);
                let dominant_type: CellType = new_counts
                    .iter()
                    .max_by_key(|entry| entry.1)
                    .map(|(spur, _)| CellType(*spur))
                    .unwrap_or(CellType::inactive());
                Some(Self {
                    width: *width,
                    history_limit: *history_limit,
                    ages,
                    cells,
                    next_cells,
                    history_data,
                    history_heads,
                    history_counts,
                    step: *step,
                    seed: *seed,
                    rule: rule.clone(),
                    counts_current: new_counts,
                    peak_counts: new_peak_counts,
                    inactive: CellType::inactive(),
                    dominant_type,
                })
            }
            _ => None,
        }
    }
}

impl Grid2D {
    pub fn from_state(state: &GridState) -> Option<Self> {
        match state {
            GridState::D2 {
                width,
                height,
                history_limit,
                cell_states,
                step,
                seed,
                rule,
                counts_current,
                peak_counts,
                model,
            } => {
                let (new_counts, new_peak_counts) =
                    convert_map_string_to_spur(cell_states, counts_current, peak_counts);
                let next_cells: Vec<CellType> = vec![CellType::inactive(); cell_states.len()];
                let cells: Vec<CellType> = cell_states.iter().map(|c| c.current).collect();
                let ages: Vec<u32> = cell_states.iter().map(|c| c.age_in_state).collect();
                let history_data = soa_history(cell_states, *history_limit);
                let history_heads = soa_heads(cell_states, *history_limit);
                let history_counts = soa_counts(cell_states, *history_limit);
                let dominant_type: CellType = counts_current
                    .iter()
                    .max_by_key(|entry| entry.1)
                    .map(|(k, _v)| CellType::from(k.as_str()))
                    .unwrap_or(CellType::inactive());
                let mut grid = Self {
                    width: *width,
                    height: *height,
                    history_limit: *history_limit,
                    ages,
                    cells,
                    next_cells,
                    history_data,
                    history_heads,
                    history_counts,
                    step: *step,
                    seed: *seed,
                    rule: rule.clone(),
                    counts_current: new_counts,
                    peak_counts: new_peak_counts,
                    inactive: CellType::inactive(),
                    dominant_type,
                    model: None,
                };
                if let Some(model) = model {
                    // A model that fails validation against its own snapshot is
                    // a malformed state; treat it like a dimension mismatch.
                    grid.attach_model(model.clone()).ok()?;
                }
                Some(grid)
            }
            _ => None,
        }
    }
}

pub fn grid2d_to_json(g: &Grid2D) -> String {
    GridState::from_grid2d(g).to_json_pretty()
}

fn convert_map_string_to_spur(
    cells: &Vec<CellState>,
    counts_current: &HashMap<String, u64>,
    peak_counts: &HashMap<String, u64>,
) -> (HashMap<Spur, u64>, HashMap<Spur, u64>) {
    let mut new_counts: HashMap<Spur, u64> = HashMap::new();
    let mut new_peak_counts: HashMap<Spur, u64> = HashMap::new();
    if counts_current.is_empty() {
        for c in cells {
            *new_counts.entry(c.current.0).or_insert(0) += 1;
        }
    } else {
        for (k, v) in counts_current.iter() {
            new_counts.insert(interner().get_or_intern(k), *v);
        }
    }
    if peak_counts.is_empty() {
        new_peak_counts = new_counts.clone();
    } else {
        for (k, v) in peak_counts.iter() {
            new_peak_counts.insert(interner().get_or_intern(k), *v);
        }
    }
    (new_counts, new_peak_counts)
}

fn convert_map_spur_to_string(
    counts_current: &HashMap<Spur, u64>,
    peak_counts: &HashMap<Spur, u64>,
) -> (HashMap<String, u64>, HashMap<String, u64>) {
    let mut new_counts: HashMap<String, u64> = HashMap::new();
    let mut new_peak_counts: HashMap<String, u64> = HashMap::new();
    for (k, v) in counts_current.iter() {
        new_counts.insert(interner().resolve(k).to_string(), *v);
    }
    for (k, v) in peak_counts.iter() {
        new_peak_counts.insert(interner().resolve(k).to_string(), *v);
    }
    (new_counts, new_peak_counts)
}

/// Convert deserialized CellState vectors to SoA history arrays.
pub(crate) fn soa_history(states: &[CellState], limit: usize) -> Vec<CellType> {
    if limit == 0 {
        return Vec::new();
    }
    let mut data = vec![CellType::inactive(); states.len() * limit];
    for (i, cs) in states.iter().enumerate() {
        let base = i * limit;
        for (j, ct) in cs.history.iter().enumerate() {
            data[base + j] = *ct;
        }
    }
    data
}
pub(crate) fn soa_heads(states: &[CellState], limit: usize) -> Vec<u8> {
    if limit == 0 {
        return Vec::new();
    }
    states
        .iter()
        .map(|cs| (cs.history.len() % limit) as u8)
        .collect()
}
pub(crate) fn soa_counts(states: &[CellState], limit: usize) -> Vec<u8> {
    if limit == 0 {
        return Vec::new();
    }
    states.iter().map(|cs| cs.history.len() as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_state_rebuilds_counts_when_serialized_maps_are_empty() {
        let a = CellType::from("A");
        let rule = Rule1D { subrules: vec![] };
        let mut g = Grid1D::new(3, 2, vec![a.clone(), a.clone(), CellType::inactive()], rule);
        g.counts_current.clear();
        g.peak_counts.clear();

        let state = GridState::from_grid1d(&g);
        let restored = Grid1D::from_state(&state).expect("restore 1D grid");
        assert_eq!(restored.counts_current.get(&a.0), Some(&2));
        assert_eq!(restored.peak_counts.get(&a.0), Some(&2));
    }

    #[test]
    fn from_state_rebuilds_counts_when_serialized_maps_are_empty_2d() {
        let a = CellType::from("A");
        let rule = Rule2D { subrules: vec![] };
        let mut g = Grid2D::new(
            2,
            2,
            2,
            vec![
                a.clone(),
                a.clone(),
                CellType::inactive(),
                CellType::inactive(),
            ],
            rule,
        );
        g.counts_current.clear();
        g.peak_counts.clear();

        let state = GridState::from_grid2d(&g);
        let restored = Grid2D::from_state(&state).expect("restore 2D grid");
        assert_eq!(restored.counts_current.get(&a.0), Some(&2));
        assert_eq!(restored.peak_counts.get(&a.0), Some(&2));
    }

    #[test]
    fn json_helpers_and_mismatched_from_state_paths_are_covered() {
        let a = CellType::from("A");
        let g2 = Grid2D::new(1, 1, 0, vec![a], Rule2D { subrules: vec![] });
        let pretty = GridState::from_grid2d(&g2).to_json_pretty();
        assert!(pretty.contains("D2"));

        let via_fn = grid2d_to_json(&g2);
        assert!(via_fn.contains("D2"));

        let s2 = GridState::from_grid2d(&g2);
        assert!(Grid1D::from_state(&s2).is_none());

        let g1 = Grid1D::new(1, 0, vec![a], Rule1D { subrules: vec![] });
        let s1 = GridState::from_grid1d(&g1);
        assert!(Grid2D::from_state(&s1).is_none());
    }
}
