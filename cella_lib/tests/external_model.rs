//! Integration tests for the external-model plugin seam and the wildfire
//! model driven through the public `Grid2D` API.
//!
//! `TestModel` below implements `ExternalModel` from outside the library,
//! proving the seam works for downstream crates.

use cella_lib::external::{
    ChunkCtx, ExternalModel, GridView, ModelError, ModelEvent, ParamDesc, ParamKind, ParamValue,
};
use cella_lib::wildfire::{
    FuelClass, SpottingParams, WildfireEnv, WildfireModel, WildfireParams, cell_rand,
};
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
    /// One tunable parameter per `ParamKind`, and nothing else. They do not
    /// change how the model steps; they exist so the acceptance test can drive
    /// every kind of control through `Grid2D::set_model_param` without the
    /// engine knowing anything about this model. `attach` refuses a `rate`
    /// above 0.9, which is what makes the rollback path reachable.
    rate: f64,
    steps: i64,
    enabled: bool,
    mode: String,
}

fn test_model(event_target: Option<usize>) -> TestModel {
    TestModel {
        event_target,
        rate: 0.5,
        steps: 3,
        enabled: false,
        mode: "Fast".into(),
    }
}

#[typetag::serde(name = "integration_test_model")]
impl ExternalModel for TestModel {
    fn attach(&mut self, view: &GridView<'_>) -> Result<(), ModelError> {
        if view.width * view.height == 0 {
            return Err(ModelError::InvalidParam("empty grid".into()));
        }
        if self.rate > 0.9 {
            return Err(ModelError::InvalidParam(
                "rate above 0.9 is rejected by attach".into(),
            ));
        }
        Ok(())
    }

    fn params(&self) -> Vec<ParamDesc> {
        let desc = |key: &str, kind: ParamKind, reattach: bool, read_only: bool| ParamDesc {
            key: key.into(),
            label: key.into(),
            group: None,
            help: None,
            unit: None,
            kind,
            reattach,
            read_only,
        };
        vec![
            desc(
                "rate",
                ParamKind::Float {
                    min: 0.0,
                    max: 1.0,
                    step: 0.05,
                },
                true,
                false,
            ),
            desc("steps", ParamKind::Int { min: 1, max: 10 }, false, false),
            desc("enabled", ParamKind::Bool, false, false),
            desc(
                "mode",
                ParamKind::Choice {
                    options: vec!["Fast".into(), "Slow".into()],
                },
                false,
                false,
            ),
            // Shown but not editable: set once when the model is built.
            desc(
                "event_target",
                ParamKind::Int { min: -1, max: 8 },
                false,
                true,
            ),
        ]
    }

    fn get_param(&self, key: &str) -> Option<ParamValue> {
        match key {
            "rate" => Some(ParamValue::Float(self.rate)),
            "steps" => Some(ParamValue::Int(self.steps)),
            "enabled" => Some(ParamValue::Bool(self.enabled)),
            "mode" => Some(ParamValue::Choice(self.mode.clone())),
            "event_target" => Some(ParamValue::Int(self.event_target.map_or(-1, |t| t as i64))),
            _ => None,
        }
    }

    fn set_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError> {
        match (key, value) {
            ("rate", ParamValue::Float(v)) => self.rate = v,
            ("steps", ParamValue::Int(v)) => self.steps = v,
            ("enabled", ParamValue::Bool(v)) => self.enabled = v,
            ("mode", ParamValue::Choice(v)) => self.mode = v,
            _ => {
                return Err(ModelError::InvalidParam(format!(
                    "unknown parameter '{key}'"
                )));
            }
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
                vec![ModelEvent {
                    target: t,
                    new_type: CellType::new("Marked"),
                }]
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

// ─── Acceptance: a generic caller tunes an out-of-tree model ───

/// Grid with the out-of-tree `TestModel` attached, ready for parameter edits.
fn param_grid() -> Grid2D {
    let a = CellType::new("A");
    let mut g = Grid2D::new(2, 2, 0, vec![a; 4], empty_rule());
    g.attach_model(Box::new(test_model(None))).unwrap();
    g
}

#[test]
fn out_of_tree_params_round_trip_through_the_engine() {
    let mut g = param_grid();
    g.set_model_param("rate", ParamValue::Float(0.75)).unwrap();
    g.set_model_param("steps", ParamValue::Int(7)).unwrap();
    g.set_model_param("enabled", ParamValue::Bool(true))
        .unwrap();
    g.set_model_param("mode", ParamValue::Choice("Slow".into()))
        .unwrap();
    let m = g.model_mut().unwrap();
    assert_eq!(m.get_param("rate"), Some(ParamValue::Float(0.75)));
    assert_eq!(m.get_param("steps"), Some(ParamValue::Int(7)));
    assert_eq!(m.get_param("enabled"), Some(ParamValue::Bool(true)));
    assert_eq!(m.get_param("mode"), Some(ParamValue::Choice("Slow".into())));
    assert_eq!(m.get_param("nothing_like_this"), None);
    // What a generic panel would draw: one control per descriptor, no
    // model-specific code anywhere.
    let keys: Vec<String> = m.params().into_iter().map(|d| d.key).collect();
    assert_eq!(keys, ["rate", "steps", "enabled", "mode", "event_target"]);
    // The model still steps after the edits.
    g.step();
    assert_eq!(
        g.cell_type(0),
        CellType::new("B"),
        "the A -> B cycle still runs"
    );
}

#[test]
fn out_of_tree_param_bounds_are_enforced_for_every_kind() {
    let mut g = param_grid();
    // Float: below, above, and not-a-number.
    assert!(g.set_model_param("rate", ParamValue::Float(-0.1)).is_err());
    assert!(g.set_model_param("rate", ParamValue::Float(1.1)).is_err());
    assert!(
        g.set_model_param("rate", ParamValue::Float(f64::NAN))
            .is_err()
    );
    // Int: below and above the declared range.
    assert!(g.set_model_param("steps", ParamValue::Int(0)).is_err());
    assert!(g.set_model_param("steps", ParamValue::Int(11)).is_err());
    // Choice: a name that is not on the list.
    assert!(
        g.set_model_param("mode", ParamValue::Choice("Sideways".into()))
            .is_err()
    );
    // A value of the wrong kind for the control.
    assert!(g.set_model_param("enabled", ParamValue::Int(1)).is_err());
    assert!(g.set_model_param("rate", ParamValue::Bool(true)).is_err());
    // An unknown key is refused, and the message names it.
    let err = g
        .set_model_param("no_such_param", ParamValue::Int(1))
        .unwrap_err();
    assert!(matches!(err, ModelError::InvalidParam(_)));
    assert!(err.to_string().contains("no_such_param"), "{err}");
    // Nothing above was written.
    let m = g.model_mut().unwrap();
    assert_eq!(m.get_param("rate"), Some(ParamValue::Float(0.5)));
    assert_eq!(m.get_param("steps"), Some(ParamValue::Int(3)));
    assert_eq!(m.get_param("enabled"), Some(ParamValue::Bool(false)));
    assert_eq!(m.get_param("mode"), Some(ParamValue::Choice("Fast".into())));
}

#[test]
fn out_of_tree_param_rolls_back_when_attach_rejects_and_read_only_is_refused() {
    let mut g = param_grid();
    // 0.95 is inside the descriptor's 0..=1, so the engine's bounds check
    // passes it on, but TestModel::attach refuses anything above 0.9.
    let err = g
        .set_model_param("rate", ParamValue::Float(0.95))
        .unwrap_err();
    assert!(matches!(err, ModelError::InvalidParam(_)));
    assert_eq!(
        g.model_mut().unwrap().get_param("rate"),
        Some(ParamValue::Float(0.5)),
        "the previous value was put back"
    );
    // The grid is still attached and steps as before.
    g.step();
    assert_eq!(g.step, 1);

    // A read-only parameter is refused before anything is written.
    let err = g
        .set_model_param("event_target", ParamValue::Int(2))
        .unwrap_err();
    assert!(
        err.to_string().contains("read-only"),
        "says why it was refused: {err}"
    );
    assert_eq!(
        g.model_mut().unwrap().get_param("event_target"),
        Some(ParamValue::Int(-1)),
        "a read-only parameter is never written"
    );
}

fn counts_total(g: &Grid2D) -> u64 {
    g.counts_current.values().sum()
}

#[test]
fn out_of_tree_model_drives_step_ages_history_and_counts() {
    let (a, b, c) = (CellType::new("A"), CellType::new("B"), CellType::new("C"));
    let init = vec![a, b, c, CellType::inactive()];
    let mut g = Grid2D::new(2, 2, 3, init, empty_rule());
    g.attach_model(Box::new(test_model(None))).unwrap();
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
    g.attach_model(Box::new(test_model(Some(4)))).unwrap();
    g.step();
    assert_eq!(
        g.cell_type(4),
        CellType::new("Marked"),
        "event overwrote the chunk result"
    );
    assert_eq!(g.cell_type(0), CellType::new("B"));
    assert_eq!(g.cell_age(4), 0, "event resets age");
    assert_eq!(
        counts_total(&g),
        9,
        "counts stay a partition after event fixup"
    );
    assert_eq!(
        g.counts_current.get(&CellType::new("Marked").0).copied(),
        Some(1)
    );
}

#[test]
fn attach_model_error_propagates_and_leaves_grid_modelless() {
    let mut g = Grid2D::new(1, 1, 0, vec![CellType::inactive()], empty_rule());
    let bad = WildfireModel::new(
        WildfireParams {
            seed: 1,
            p0: 2.0, // invalid
            fuels: vec![FuelClass {
                name: "F".into(),
                veg_factor: 1.0,
            }],
            wind_speed: 0.0,
            wind_from_deg: 270.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 1,
            spotting: None,
            burning_name: None,
            burned_name: None,
            spread: "bernoulli".into(),
            arrival_jitter: 0.2,
            wind_law: "exponential".into(),
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
            FuelClass {
                name: "Forest".into(),
                veg_factor: 1.0,
            },
            FuelClass {
                name: "Shrub".into(),
                veg_factor: 0.6,
            },
        ],
        wind_speed: 6.0,
        wind_from_deg: 300.0, // blows toward 30° on the grid
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
        spread: "bernoulli".into(),
        arrival_jitter: 0.2,
        wind_law: "exponential".into(),
    }
}

/// Mixed-fuel grid with a slope ramp and a centre ignition.
fn wildfire_grid(w: usize, h: usize, seed: u64, history_limit: usize) -> Grid2D {
    let forest = CellType::new("Forest");
    let shrub = CellType::new("Shrub");
    let mut init = vec![forest; w * h];
    for (idx, cell) in init.iter_mut().enumerate() {
        // Deterministic fuel mosaic with some unburnable water cells.
        match cell_rand(999, 0, idx as u64, 7) {
            v if v < 0.2 => *cell = shrub,
            v if v < 0.25 => *cell = CellType::inactive(),
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
        WildfireEnv {
            density: vec![],
            elevation,
            ..Default::default()
        },
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
    params.fuels = vec![FuelClass {
        name: "Forest".into(),
        veg_factor: 1.0,
    }];
    let mut g = Grid2D::new(5, 5, 0, init, empty_rule());
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
        .unwrap();
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
    assert_eq!(
        burning, 0,
        "everything reachable has burned out after 41 steps"
    );
    assert!(
        g.counts_current
            .get(&CellType::new("BurnedOut").0)
            .copied()
            .unwrap_or(0)
            >= 13
    );
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
        assert_eq!(
            counts_total(&serial),
            31 * 23,
            "counts partition every step"
        );
    }
    for threads in [2usize, 4, 8] {
        cella_lib::threads::set_thread_override(threads);
        let mut par = wildfire_grid(31, 23, 7, 3);
        for _ in 0..steps {
            par.step();
        }
        for idx in 0..31 * 23 {
            assert_eq!(
                par.cell_type(idx),
                serial.cell_type(idx),
                "cells t={threads} idx={idx}"
            );
            assert_eq!(
                par.cell_age(idx),
                serial.cell_age(idx),
                "ages t={threads} idx={idx}"
            );
            assert_eq!(
                par.cell_history(idx),
                serial.cell_history(idx),
                "history t={threads} idx={idx}"
            );
        }
        assert_eq!(
            par.counts_current, serial.counts_current,
            "counts t={threads}"
        );
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
    params.wind_from_deg = 270.0; // west wind: firebrands fly east
    params.burn_duration = 10;
    params.fuels = vec![FuelClass {
        name: "Forest".into(),
        veg_factor: 1.0,
    }];
    params.spotting = Some(SpottingParams {
        p_spot: 1.0,
        median_distance: 3.0,
        sigma: 0.0,
        angle_jitter_deg: 0.0,
    });
    let mut g = Grid2D::new(9, 9, 0, init, empty_rule());
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
        .unwrap();
    g.step();
    assert_eq!(
        g.cell_type(4 * 9 + 4),
        b,
        "firebrand landed 3 cells downwind"
    );
    assert_eq!(
        g.cell_type(4 * 9 + 1),
        b,
        "source still burning (duration 10)"
    );
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
    params.fuels = vec![FuelClass {
        name: "Forest".into(),
        veg_factor: 1.0,
    }];
    let mut g = Grid2D::new(3, 3, 0, init, empty_rule());
    g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
        .unwrap();
    assert!(
        g.transition_state_and_buffer(1, &CellType::inactive())
            .is_none()
    );
    g.step();
    assert_eq!(
        g.cell_type(1),
        CellType::inactive(),
        "painted-out cell cannot ignite"
    );
    assert_eq!(g.cell_type(3), b, "unpainted cardinal ignites at p0 = 1");
    // Paint fuel back in; it ignites on the next step.
    assert!(g.transition_state_and_buffer(1, &f).is_none());
    g.step();
    assert_eq!(g.cell_type(1), b, "restored fuel ignites");
}

#[test]
fn model_mut_downcast_adjusts_wind_between_steps() {
    let mut g = wildfire_grid(10, 10, 1, 0);
    let m = g
        .model_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<WildfireModel>()
        .unwrap();
    m.params.wind_speed = 99.0;
    m.params.wind_from_deg = 90.0;
    g.step();
    let m = g
        .model_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<WildfireModel>()
        .unwrap();
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
        assert_eq!(
            reference.cell_type(idx),
            restored.cell_type(idx),
            "idx {idx}"
        );
        assert_eq!(reference.cell_age(idx), restored.cell_age(idx), "age {idx}");
    }
    assert_eq!(reference.counts_current, restored.counts_current);
}

#[test]
fn gridstate_without_model_still_round_trips() {
    let alive = CellType::new("Alive");
    let g = Grid2D::new(
        2,
        2,
        1,
        vec![alive, alive, CellType::inactive(), CellType::inactive()],
        empty_rule(),
    );
    let json = GridState::from_grid2d(&g).to_json_pretty();
    assert!(
        !json.contains("\"model\""),
        "no model field serialized when absent"
    );
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
    assert!(
        cfg.build_grid2d().is_none(),
        "invalid model p0 rejects the build"
    );
}

// ─── Packed 1D Wolfram fast path: exact equivalence with the scalar path ───

mod packed_wolfram {
    use cella_lib::{CellType, Grid1D, Rule1D, Rule1DSubrule};

    fn wolfram_rule(code: u128, extra_dummy: bool) -> Rule1D {
        let x = CellType::from("X");
        let inactive = CellType::inactive();
        let sub = |current: CellType| Rule1DSubrule {
            current_type: current,
            criteria_type: x,
            wolfram_code: code,
            n: 1,
            randomness: None,
            output_type: x,
        };
        let mut subrules = vec![sub(x), sub(inactive)];
        if extra_dummy {
            // A third, unreachable duplicate defeats the packed-shape detection
            // without changing semantics (first match wins) — this is how the
            // reference grid is forced onto the scalar path.
            subrules.push(sub(inactive));
        }
        Rule1D { subrules }
    }

    fn seeded_init(width: usize, seed: u64) -> Vec<CellType> {
        let x = CellType::from("X");
        (0..width)
            .map(|j| {
                if cella_lib::wildfire::cell_rand(seed, 0, j as u64, 0) < 0.35 {
                    x
                } else {
                    CellType::inactive()
                }
            })
            .collect()
    }

    #[test]
    fn packed_matches_scalar_exactly() {
        // Codes cover: rule 30, rule 110, a pattern-000-fires code (odd), 0 and 255.
        for code in [30u128, 110, 129, 0, 255] {
            // Widths straddle word boundaries.
            for width in [1usize, 63, 64, 65, 130, 2049] {
                for hl in [0usize, 2] {
                    let init = seeded_init(width, code as u64 ^ width as u64);
                    let mut packed =
                        Grid1D::new(width, hl, init.clone(), wolfram_rule(code, false));
                    let mut scalar = Grid1D::new(width, hl, init, wolfram_rule(code, true));
                    for step in 0..12 {
                        packed.step();
                        scalar.step();
                        for j in 0..width {
                            assert_eq!(
                                packed.cell_type(j),
                                scalar.cell_type(j),
                                "cells code={code} w={width} hl={hl} step={step} j={j}"
                            );
                            assert_eq!(
                                packed.cell_age(j),
                                scalar.cell_age(j),
                                "ages code={code} w={width} hl={hl} step={step} j={j}"
                            );
                            assert_eq!(
                                packed.cell_history(j),
                                scalar.cell_history(j),
                                "history code={code} w={width} hl={hl} step={step} j={j}"
                            );
                        }
                        assert_eq!(
                            packed.counts_current, scalar.counts_current,
                            "counts code={code} w={width} hl={hl} step={step}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn foreign_cell_falls_back_to_scalar_semantics() {
        // Paint a third type mid-run: the packed path must decline that step
        // and the foreign cell must become inactive (no subrule matches it),
        // exactly as the scalar path dictates.
        let width = 130;
        let mut g = Grid1D::new(width, 0, seeded_init(width, 9), wolfram_rule(30, false));
        let mut reference = Grid1D::new(width, 0, seeded_init(width, 9), wolfram_rule(30, true));
        g.step();
        reference.step();
        let c = CellType::from("Foreign");
        assert!(g.transition_state_and_buffer(65, &c).is_none());
        assert!(reference.transition_state_and_buffer(65, &c).is_none());
        for step in 0..4 {
            g.step();
            reference.step();
            for j in 0..width {
                assert_eq!(g.cell_type(j), reference.cell_type(j), "step={step} j={j}");
            }
        }
        assert_eq!(
            g.cell_type(65),
            CellType::inactive(),
            "foreign type decays to inactive"
        );
    }
}

// ─── Packed 2D threshold fast path: exact equivalence with the scalar path ───

mod packed_threshold_2d {
    use cella_lib::{CellType, CountOp, Grid2D, Neighborhood2D, Rule2D, Rule2DSubrule};

    fn life_like(neighborhood: Neighborhood2D, defeat_detection: bool) -> Rule2D {
        let a = CellType::from("Alive");
        let inactive = CellType::inactive();
        let mut subrules = vec![
            Rule2DSubrule::new(a, a, 4, CountOp::Gt, 1, neighborhood, inactive, None, None),
            Rule2DSubrule::new(a, a, 2, CountOp::Gt, 1, neighborhood, a, None, None),
            Rule2DSubrule::new(inactive, a, 3, CountOp::Eq, 1, neighborhood, a, None, None),
        ];
        if defeat_detection {
            // A subrule whose current type never occurs in a two-type world:
            // semantically inert (it can never match), but its non-two-type
            // current defeats the packed-shape detection, forcing scalar.
            let ghost = CellType::from("Ghost");
            subrules.push(Rule2DSubrule::new(
                ghost,
                a,
                1,
                CountOp::Gt,
                1,
                neighborhood,
                a,
                None,
                None,
            ));
        }
        Rule2D { subrules }
    }

    fn soup(w: usize, h: usize, seed: u64) -> Vec<CellType> {
        let a = CellType::from("Alive");
        (0..w * h)
            .map(|j| {
                if cella_lib::wildfire::cell_rand(seed, 0, j as u64, 1) < 0.4 {
                    a
                } else {
                    CellType::inactive()
                }
            })
            .collect()
    }

    #[test]
    fn packed_matches_scalar_exactly() {
        for nb in [
            Neighborhood2D::Moore,
            Neighborhood2D::VonNeumann,
            Neighborhood2D::Langton,
        ] {
            for (w, h) in [
                (9usize, 7usize),
                (63, 5),
                (64, 4),
                (65, 4),
                (130, 3),
                (256, 16),
            ] {
                for hl in [0usize, 2] {
                    let init = soup(w, h, w as u64 ^ (hl as u64) << 8);
                    let mut packed = Grid2D::new(w, h, hl, init.clone(), life_like(nb, false));
                    let mut scalar = Grid2D::new(w, h, hl, init, life_like(nb, true));
                    for step in 0..10 {
                        packed.step();
                        scalar.step();
                        for j in 0..w * h {
                            assert_eq!(
                                packed.cell_type(j),
                                scalar.cell_type(j),
                                "cells nb={nb:?} w={w} h={h} hl={hl} step={step} j={j}"
                            );
                            assert_eq!(
                                packed.cell_age(j),
                                scalar.cell_age(j),
                                "ages nb={nb:?} w={w} h={h} hl={hl} step={step} j={j}"
                            );
                            assert_eq!(
                                packed.cell_history(j),
                                scalar.cell_history(j),
                                "history nb={nb:?} w={w} h={h} hl={hl} step={step} j={j}"
                            );
                        }
                        assert_eq!(
                            packed.counts_current, scalar.counts_current,
                            "counts nb={nb:?} w={w} h={h} hl={hl} step={step}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn foreign_cell_falls_back_to_scalar_semantics() {
        let (w, h) = (70, 6);
        let mut g = Grid2D::new(
            w,
            h,
            0,
            soup(w, h, 3),
            life_like(Neighborhood2D::Moore, false),
        );
        let mut reference = Grid2D::new(
            w,
            h,
            0,
            soup(w, h, 3),
            life_like(Neighborhood2D::Moore, true),
        );
        g.step();
        reference.step();
        let f = CellType::from("Foreign2D");
        assert!(g.transition_state_and_buffer(3 * w + 65, &f).is_none());
        assert!(
            reference
                .transition_state_and_buffer(3 * w + 65, &f)
                .is_none()
        );
        for step in 0..4 {
            g.step();
            reference.step();
            for j in 0..w * h {
                assert_eq!(g.cell_type(j), reference.cell_type(j), "step={step} j={j}");
            }
        }
        assert_eq!(
            g.cell_type(3 * w + 65),
            CellType::inactive(),
            "foreign type decays to inactive"
        );
    }
}

#[test]
fn existing_configs_still_load() {
    // Back-compat: every committed config (no model field) must still build.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("configs");
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
    assert!(
        checked >= 10,
        "expected the committed config corpus, found {checked}"
    );
}
