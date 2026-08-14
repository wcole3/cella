//! Integration tests for the external-model plugin seam and the wildfire
//! model driven through the public `Grid2D` API.
//!
//! `TestModel` below implements `ExternalModel` from outside the library,
//! proving the seam works for downstream crates.

use cella_lib::external::{ChunkCtx, ExternalModel, GridView, ModelError, ModelEvent};
use cella_lib::wildfire::{cell_rand, FuelClass, SpottingParams, WildfireEnv, WildfireModel, WildfireParams};
use cella_lib::{CellType, Grid2D, GridState, Rule2D};
use serde::{Deserialize, Serialize};

/// `set_thread_override` / `set_min_work_per_chunk_override` are process-global,
/// so tests that drive them must not run concurrently with each other.
static THREAD_OVERRIDE_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn empty_rule() -> Rule2D {
    Rule2D { subrules: vec![] }
}

// ─── An out-of-tree model ───

/// Cyclic model: A -> B -> C -> A each step, everything else untouched; cells
/// whose flat index equals `event_target` are overwritten via an event.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TestModel {
    event_target: Option<usize>,
}

#[typetag::serde(name = "integration_test_model")]
impl ExternalModel for TestModel {
    fn attach(&mut self, view: &GridView<'_>) -> Result<(), ModelError> {
        if view.width * view.height == 0 {
            return Err(ModelError::InvalidParam("empty grid".into()));
        }
        Ok(())
    }

    fn step_chunk(&self, ctx: &ChunkCtx<'_>, next: &mut [CellType]) -> Vec<ModelEvent> {
        let (a, b, c) = (CellType::new("A"), CellType::new("B"), CellType::new("C"));
        for (local, slot) in next.iter_mut().enumerate() {
            let cur = ctx.cells[ctx.start + local];
            *slot = if cur == a {
                b
            } else if cur == b {
                c
            } else if cur == c {
                a
            } else {
                cur
            };
        }
        match self.event_target {
            Some(t) if (ctx.start..ctx.start + next.len()).contains(&t) => {
                vec![ModelEvent { target: t, new_type: CellType::new("Marked") }]
            }
            _ => Vec::new(),
        }
    }

    fn boxed_clone(&self) -> Box<dyn ExternalModel> {
        Box::new(self.clone())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn counts_total(g: &Grid2D) -> u64 {
    g.counts_current.values().sum()
}

#[test]
fn out_of_tree_model_drives_step_ages_history_and_counts() {
    let (a, b, c) = (CellType::new("A"), CellType::new("B"), CellType::new("C"));
    let init = vec![a, b, c, CellType::inactive()];
    let mut g = Grid2D::new(2, 2, 3, init, empty_rule());
    g.attach_model(Box::new(TestModel { event_target: None })).unwrap();
    g.step();
    assert_eq!(g.cell_type(0), b);
    assert_eq!(g.cell_type(1), c);
    assert_eq!(g.cell_type(2), a);
    assert_eq!(g.cell_type(3), CellType::inactive());
    assert_eq!(g.step, 1);
    // Ages: changed cells reset, unchanged cell increments.
    assert_eq!(g.cell_age(0), 0);
    assert_eq!(g.cell_age(3), 1);
    // History recorded the previous state.
    assert_eq!(g.cell_history(0), vec![a]);
    assert_eq!(g.cell_history(3), vec![CellType::inactive()]);
    // Counts stay a partition of the grid.
    assert_eq!(counts_total(&g), 4);
    assert_eq!(g.counts_current.get(&b.0).copied(), Some(1));
}

#[test]
fn model_events_apply_through_step_and_fix_counts() {
    let a = CellType::new("A");
    let init = vec![a; 9];
    let mut g = Grid2D::new(3, 3, 0, init, empty_rule());
    g.attach_model(Box::new(TestModel { event_target: Some(4) })).unwrap();
    g.step();
    assert_eq!(g.cell_type(4), CellType::new("Marked"), "event overwrote the chunk result");
    assert_eq!(g.cell_type(0), CellType::new("B"));
    assert_eq!(g.cell_age(4), 0, "event resets age");
    assert_eq!(counts_total(&g), 9, "counts stay a partition after event fixup");
    assert_eq!(g.counts_current.get(&CellType::new("Marked").0).copied(), Some(1));
}

#[test]
fn attach_model_error_propagates_and_leaves_grid_modelless() {
    let mut g = Grid2D::new(1, 1, 0, vec![CellType::inactive()], empty_rule());
    let bad = WildfireModel::new(
        WildfireParams {
            seed: 1,
            p0: 2.0, // invalid
            fuels: vec![FuelClass { name: "F".into(), veg_factor: 1.0 }],
            wind_speed: 0.0,
            wind_dir_deg: 0.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 1,
            spotting: None,
            burning_name: None,
            burned_name: None,
        },
        WildfireEnv::default(),
    );
    assert!(g.attach_model(Box::new(bad)).is_err());
    assert!(g.model.is_none());
    g.step(); // still steps fine on the subrule path
    assert_eq!(g.step, 1);
}

// ─── Wildfire through the public API ───

fn wildfire_params(seed: u64) -> WildfireParams {
    WildfireParams {
        seed,
        p0: 0.58,
        fuels: vec![
            FuelClass { name: "Forest".into(), veg_factor: 1.0 },
            FuelClass { name: "Shrub".into(), veg_factor: 0.6 },
        ],
        wind_speed: 6.0,
        wind_dir_deg: 30.0,
        c1: 0.045,
        c2: 0.131,
        slope_a: 0.078,
        cell_size: 30.0,
        burn_duration: 2,
        spotting: Some(SpottingParams {
            p_spot: 0.05,
            median_distance: 4.0,
            sigma: 0.3,
            angle_jitter_deg: 20.0,
        }),
        burning_name: None,
        burned_name: None,
    }
}

/// Mixed-fuel grid with a slope ramp and a centre ignition.
fn wildfire_grid(w: usize, h: usize, seed: u64, history_limit: usize) -> Grid2D {
    let forest = CellType::new("Forest");
    let shrub = CellType::new("Shrub");
    let mut init = vec![forest; w * h];
    for idx in 0..w * h {
        // Deterministic fuel mosaic with some unburnable water cells.
        match cell_rand(999, 0, idx as u64, 7) {
            v if v < 0.2 => init[idx] = shrub,
            v if v < 0.25 => init[idx] = CellType::inactive(),
            _ => {}
        }
    }
    init[(h / 2) * w + w / 2] = CellType::new("Burning");
    let mut elevation = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            elevation[y * w + x] = x as f32 * 2.0 + y as f32;
        }
    }
    let mut g = Grid2D::new(w, h, history_limit, init, empty_rule());
    g.attach_model(Box::new(WildfireModel::new(
        wildfire_params(seed),
        WildfireEnv { density: vec![], elevation },
    )))
    .unwrap();
    g
}

#[test]
fn wildfire_5x5_hand_checked_spread() {
    // p0 = 1, flat, no wind: after one step the four cardinal neighbours of
    // the centre fire must burn; centre burns out after burn_duration = 1.
    let f = CellType::new("Forest");
    let b = CellType::new("Burning");
    let mut init = vec![f; 25];
    init[12] = b;
    let mut params = wildfire_params(3);
    params.p0 = 1.0;
    params.wind_speed = 0.0;
    params.burn_duration = 1;
    params.spotting = None;
    params.fuels = vec![FuelClass { name: "Forest".into(), veg_factor: 1.0 }];
    let mut g = Grid2D::new(5, 5, 0, init, empty_rule());
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default()))).unwrap();
    g.step();
    for idx in [7usize, 11, 13, 17] {
        assert_eq!(g.cell_type(idx), b, "cardinal neighbour {idx}");
    }
    assert_eq!(g.cell_type(12), CellType::new("BurnedOut"));
    assert_eq!(counts_total(&g), 25);
    // Fire keeps advancing and eventually burns out; the run terminates.
    for _ in 0..40 {
        g.step();
    }
    let burning = g.counts_current.get(&b.0).copied().unwrap_or(0);
    assert_eq!(burning, 0, "everything reachable has burned out after 41 steps");
    assert!(g.counts_current.get(&CellType::new("BurnedOut").0).copied().unwrap_or(0) >= 13);
}

#[test]
fn wildfire_identical_runs_are_identical() {
    let mut a = wildfire_grid(24, 24, 42, 2);
    let mut b = wildfire_grid(24, 24, 42, 2);
    for _ in 0..30 {
        a.step();
        b.step();
    }
    for idx in 0..24 * 24 {
        assert_eq!(a.cell_type(idx), b.cell_type(idx));
        assert_eq!(a.cell_age(idx), b.cell_age(idx));
    }
    assert_eq!(a.counts_current, b.counts_current);
    // Different seed diverges (sanity that randomness is doing something).
    let mut c = wildfire_grid(24, 24, 43, 2);
    for _ in 0..30 {
        c.step();
    }
    let diverged = (0..24 * 24).any(|i| a.cell_type(i) != c.cell_type(i));
    assert!(diverged, "different seeds must diverge");
}

#[test]
fn wildfire_parallel_matches_serial_exactly() {
    let _guard = THREAD_OVERRIDE_GUARD.lock().unwrap();
    cella_lib::threads::set_min_work_per_chunk_override(1);
    let steps = 40;
    cella_lib::threads::set_thread_override(1);
    let mut serial = wildfire_grid(31, 23, 7, 3); // deliberately not thread-divisible
    for _ in 0..steps {
        serial.step();
        assert_eq!(counts_total(&serial), 31 * 23, "counts partition every step");
    }
    for threads in [2usize, 4, 8] {
        cella_lib::threads::set_thread_override(threads);
        let mut par = wildfire_grid(31, 23, 7, 3);
        for _ in 0..steps {
            par.step();
        }
        for idx in 0..31 * 23 {
            assert_eq!(par.cell_type(idx), serial.cell_type(idx), "cells t={threads} idx={idx}");
            assert_eq!(par.cell_age(idx), serial.cell_age(idx), "ages t={threads} idx={idx}");
            assert_eq!(par.cell_history(idx), serial.cell_history(idx), "history t={threads} idx={idx}");
        }
        assert_eq!(par.counts_current, serial.counts_current, "counts t={threads}");
        assert_eq!(par.peak_counts, serial.peak_counts, "peaks t={threads}");
    }
    cella_lib::threads::clear_thread_override();
    cella_lib::threads::clear_min_work_per_chunk_override();
}

#[test]
fn wildfire_spotting_lands_through_step() {
    // Deterministic spot: p_spot = 1, sigma = 0, no jitter, wind due east,
    // median 3 cells -> the cell 3 east of the fire ignites through step().
    let f = CellType::new("Forest");
    let b = CellType::new("Burning");
    let mut init = vec![f; 81];
    init[4 * 9 + 1] = b;
    let mut params = wildfire_params(11);
    params.p0 = 0.0; // isolate spotting
    params.wind_dir_deg = 0.0;
    params.burn_duration = 10;
    params.fuels = vec![FuelClass { name: "Forest".into(), veg_factor: 1.0 }];
    params.spotting = Some(SpottingParams { p_spot: 1.0, median_distance: 3.0, sigma: 0.0, angle_jitter_deg: 0.0 });
    let mut g = Grid2D::new(9, 9, 0, init, empty_rule());
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default()))).unwrap();
    g.step();
    assert_eq!(g.cell_type(4 * 9 + 4), b, "firebrand landed 3 cells downwind");
    assert_eq!(g.cell_type(4 * 9 + 1), b, "source still burning (duration 10)");
    assert_eq!(counts_total(&g), 81);
}

#[test]
fn paint_updates_model_derived_state() {
    // Painting Inactive onto fuel makes it unignitable even at p0 = 1;
    // painting fuel back restores ignitability.
    let f = CellType::new("Forest");
    let b = CellType::new("Burning");
    let mut init = vec![f; 9];
    init[4] = b;
    let mut params = wildfire_params(5);
    params.p0 = 1.0;
    params.wind_speed = 0.0;
    params.burn_duration = 100;
    params.spotting = None;
    params.fuels = vec![FuelClass { name: "Forest".into(), veg_factor: 1.0 }];
    let mut g = Grid2D::new(3, 3, 0, init, empty_rule());
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default()))).unwrap();
    assert!(g.transition_state_and_buffer(1, &CellType::inactive()).is_none());
    g.step();
    assert_eq!(g.cell_type(1), CellType::inactive(), "painted-out cell cannot ignite");
    assert_eq!(g.cell_type(3), b, "unpainted cardinal ignites at p0 = 1");
    // Paint fuel back in; it ignites on the next step.
    assert!(g.transition_state_and_buffer(1, &f).is_none());
    g.step();
    assert_eq!(g.cell_type(1), b, "restored fuel ignites");
}

#[test]
fn model_mut_downcast_adjusts_wind_between_steps() {
    let mut g = wildfire_grid(10, 10, 1, 0);
    let m = g.model_mut().unwrap().as_any_mut().downcast_mut::<WildfireModel>().unwrap();
    m.params.wind_speed = 99.0;
    m.params.wind_dir_deg = 180.0;
    g.step();
    let m = g.model_mut().unwrap().as_any_mut().downcast_mut::<WildfireModel>().unwrap();
    assert_eq!(m.params.wind_speed, 99.0);
}

#[test]
fn gridstate_round_trip_with_model_continues_identically() {
    let mut reference = wildfire_grid(16, 16, 21, 2);
    let mut restored_src = wildfire_grid(16, 16, 21, 2);
    for _ in 0..10 {
        reference.step();
        restored_src.step();
    }
    // Snapshot mid-run, restore, and keep stepping both.
    let json = GridState::from_grid2d(&restored_src).to_json_pretty();
    let mut restored = Grid2D::from_state(&GridState::from_json(&json).unwrap()).unwrap();
    assert!(restored.model.is_some(), "model survives the snapshot");
    for _ in 0..10 {
        reference.step();
        restored.step();
    }
    for idx in 0..16 * 16 {
        assert_eq!(reference.cell_type(idx), restored.cell_type(idx), "idx {idx}");
        assert_eq!(reference.cell_age(idx), restored.cell_age(idx), "age {idx}");
    }
    assert_eq!(reference.counts_current, restored.counts_current);
}

#[test]
fn gridstate_without_model_still_round_trips() {
    let alive = CellType::new("Alive");
    let g = Grid2D::new(2, 2, 1, vec![alive, alive, CellType::inactive(), CellType::inactive()], empty_rule());
    let json = GridState::from_grid2d(&g).to_json_pretty();
    assert!(!json.contains("\"model\""), "no model field serialized when absent");
    let restored = Grid2D::from_state(&GridState::from_json(&json).unwrap()).unwrap();
    assert!(restored.model.is_none());
}

#[test]
fn config_with_wildfire_model_builds_and_runs() {
    let json = r#"{
        "dim": "2d",
        "width": 4,
        "height": 4,
        "history_limit": 0,
        "initial": ["Forest","Forest","Forest","Forest",
                    "Forest","Burning","Forest","Forest",
                    "Forest","Forest","Forest","Forest",
                    "Forest","Forest","Forest","Inactive"],
        "rule": { "subrules": [] },
        "model": { "wildfire": { "params": {
            "seed": 9,
            "p0": 1.0,
            "fuels": [ { "name": "Forest", "veg_factor": 1.0 } ],
            "burn_duration": 1
        } } }
    }"#;
    let cfg: cella_lib::config::CellaConfig = serde_json::from_str(json).unwrap();
    let mut g = cfg.build_grid2d().expect("model config builds");
    assert!(g.model.is_some());
    g.step();
    assert_eq!(g.cell_type(5), CellType::new("BurnedOut"));
    assert_eq!(g.cell_type(1), CellType::new("Burning"));
    // Round-trip the config itself.
    let round = serde_json::to_string(&cfg).unwrap();
    let cfg2: cella_lib::config::CellaConfig = serde_json::from_str(&round).unwrap();
    assert!(cfg2.build_grid2d().is_some());
}

#[test]
fn config_with_invalid_model_returns_none() {
    let json = r#"{
        "dim": "2d",
        "width": 2, "height": 2, "history_limit": 0,
        "initial": ["Forest","Forest","Forest","Forest"],
        "rule": { "subrules": [] },
        "model": { "wildfire": { "params": { "seed": 1, "p0": 5.0,
            "fuels": [ { "name": "Forest", "veg_factor": 1.0 } ] } } }
    }"#;
    let cfg: cella_lib::config::CellaConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.build_grid2d().is_none(), "invalid model p0 rejects the build");
}

#[test]
fn existing_configs_still_load() {
    // Back-compat: every committed config (no model field) must still build.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("configs");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        if entry.path().extension().is_some_and(|e| e == "json") {
            let cfg = cella_lib::config::CellaConfig::from_file(entry.path())
                .unwrap_or_else(|e| panic!("{}: {e}", entry.path().display()));
            match &cfg {
                cella_lib::config::CellaConfig::D1(_) => assert!(cfg.build_grid1d().is_some()),
                cella_lib::config::CellaConfig::D2(_) => assert!(cfg.build_grid2d().is_some()),
            }
            checked += 1;
        }
    }
    assert!(checked >= 10, "expected the committed config corpus, found {checked}");
}
