//! The explore engines driven entirely from outside the library: an
//! out-of-tree model and an out-of-tree driver, registered with typetag the
//! way a downstream crate would, configured through JSON, run through
//! `CellaConfig::build_ensemble` / `build_evolution`.
//!
//! If this compiles and passes, nothing in `cella_lib::explore` depends on
//! the wildfire model.

use cella_lib::CellType;
use cella_lib::config::CellaConfig;
use cella_lib::explore::driver::{Forcing, MemberDriver, MemberState};
use cella_lib::explore::genome::{Gene, GeneSpace, Genome};
use cella_lib::explore::{Metric, Sim};
use cella_lib::external::{
    ChunkCtx, ExternalModel, GridView, ModelError, ModelEvent, ParamDesc, ParamKind, ParamValue,
};
use cella_lib::rng::Rng;
use serde::{Deserialize, Serialize};

// ─── An out-of-tree model with one knob and a seed ───

/// Cyclic model A -> B -> C -> A; a cell advances only when a seeded draw
/// falls below `rate`, so `rate` and the seed both matter.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CycleModel {
    rate: f64,
    #[serde(default)]
    seed: u64,
}

#[typetag::serde(name = "explore_test_model")]
impl ExternalModel for CycleModel {
    fn attach(&mut self, view: &GridView<'_>) -> Result<(), ModelError> {
        if view.width * view.height == 0 {
            return Err(ModelError::InvalidParam("empty grid".into()));
        }
        Ok(())
    }

    fn declared_types(&self) -> Vec<CellType> {
        vec![CellType::new("A"), CellType::new("B"), CellType::new("C")]
    }

    fn params(&self) -> Vec<ParamDesc> {
        vec![ParamDesc {
            key: "rate".into(),
            label: "Advance rate".into(),
            group: None,
            help: None,
            unit: None,
            kind: ParamKind::Float {
                min: 0.0,
                max: 1.0,
                step: 0.01,
            },
            reattach: false,
            read_only: false,
        }]
    }

    fn get_param(&self, key: &str) -> Option<ParamValue> {
        (key == "rate").then_some(ParamValue::Float(self.rate))
    }

    fn set_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError> {
        match (key, value) {
            ("rate", ParamValue::Float(v)) => {
                self.rate = v;
                Ok(())
            }
            _ => Err(ModelError::InvalidParam(format!(
                "unknown parameter '{key}'"
            ))),
        }
    }

    fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
    }

    fn step_chunk(&self, ctx: &ChunkCtx<'_>, next: &mut [CellType]) -> Vec<ModelEvent> {
        let (a, b, c) = (CellType::new("A"), CellType::new("B"), CellType::new("C"));
        for (local, slot) in next.iter_mut().enumerate() {
            let idx = ctx.start + local;
            let cur = ctx.cells[idx];
            let advance = f64::from(cella_lib::rng::cell_rand(
                self.seed, ctx.step, idx as u64, 0,
            )) < self.rate;
            *slot = if !advance {
                cur
            } else if cur == a {
                b
            } else if cur == b {
                c
            } else if cur == c {
                a
            } else {
                cur
            };
        }
        Vec::new()
    }

    fn boxed_clone(&self) -> Box<dyn ExternalModel> {
        Box::new(self.clone())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ─── An out-of-tree driver ───

/// Owns `model.rate`: writes `boost × level` (a free gene times a forcing
/// value) into it, and counts period boundaries in the member state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct BoostDriver {
    #[serde(default = "two")]
    period: u64,
}

fn two() -> u64 {
    2
}

#[typetag::serde(name = "explore_test_driver")]
impl MemberDriver for BoostDriver {
    fn free_genes(&self) -> Vec<Gene> {
        vec![Gene::float("boost", 0.0, 2.0, false)]
    }

    fn owned_keys(&self) -> Vec<String> {
        vec!["model.rate".into()]
    }

    fn period_steps(&self) -> Option<u64> {
        Some(self.period)
    }

    fn apply(
        &self,
        sim: &mut Sim,
        genome: &Genome,
        space: &GeneSpace,
        forcing: &Forcing,
        state: &mut MemberState,
    ) -> Result<(), ModelError> {
        let boost = space.float(genome, "boost").unwrap_or(1.0);
        let level = forcing.get("level").copied().unwrap_or(0.5);
        let rate = (boost * level).clamp(0.0, 1.0);
        state.set("rate", rate);
        sim.set_param("model.rate", ParamValue::Float(rate))
    }

    fn period_end(
        &self,
        _sim: &mut Sim,
        _genome: &Genome,
        _space: &GeneSpace,
        state: &mut MemberState,
        _rng: &mut Rng,
    ) -> Result<(), ModelError> {
        state.set("periods", state.get("periods").unwrap_or(0.0) + 1.0);
        Ok(())
    }

    fn boxed_clone(&self) -> Box<dyn MemberDriver> {
        Box::new(self.clone())
    }
}

fn config_json(extra: &str) -> String {
    let initial: Vec<&str> = (0..36)
        .map(|i| ["A", "B", "C", "Inactive"][i % 4])
        .collect();
    format!(
        r#"{{
            "dim": "2d", "width": 6, "height": 6, "history_limit": 0, "seed": 5,
            "initial": {},
            "rule": {{"subrules": []}},
            "model": {{"explore_test_model": {{"rate": 0.5}}}},
            {extra}
        }}"#,
        serde_json::to_string(&initial).unwrap()
    )
}

#[test]
fn an_out_of_tree_model_and_driver_run_as_an_ensemble_from_json() {
    let json = config_json(
        r#""ensemble": {
            "members": 6, "seed": 2,
            "genes": [{"key": "boost", "range": [0.5, 1.5]}, {"key": "model.rate"}],
            "track": ["A"],
            "driver": {"explore_test_driver": {"period": 3}}
        }"#,
    );
    let cfg: CellaConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.ensemble().unwrap().members, 6);
    let mut ens = cfg
        .build_ensemble()
        .expect("block present")
        .expect("block valid");
    assert_eq!(ens.len(), 6);
    assert_eq!(ens.track(), &[CellType::new("A")]);
    // The driver owns model.rate: every member's rate is boost × 0.5, not the gene draw.
    for m in ens.members() {
        let boost = ens.space().float(&m.genome, "boost").unwrap();
        let rate = m.sim.get_param("model.rate").unwrap();
        assert_eq!(rate, ParamValue::Float((boost * 0.5).clamp(0.0, 1.0)));
        assert_eq!(m.state.get("rate"), Some((boost * 0.5).clamp(0.0, 1.0)));
        assert_eq!(m.sim.seed(), m.seed, "grid reseeded per member");
    }
    // set_seed reached the model: two members with the same rate still differ.
    let mut f = Forcing::new();
    f.insert("level".into(), 1.0);
    ens.set_forcing(f).unwrap();
    for m in ens.members() {
        let boost = ens.space().float(&m.genome, "boost").unwrap();
        assert_eq!(
            m.sim.get_param("model.rate"),
            Some(ParamValue::Float(boost.min(1.0)))
        );
    }
    ens.step_n(6).unwrap();
    assert_eq!(ens.step_count(), 6);
    let p = ens.state_probability(&[CellType::new("A")]);
    assert!(
        p.iter().any(|&v| v > 0.0 && v < 1.0),
        "members disagree: {p:?}"
    );
    // Two period boundaries (steps 3 and 6) were counted by the driver.
    assert!(
        ens.members()
            .iter()
            .all(|m| m.state.get("periods") == Some(2.0))
    );
    assert_eq!(ens.state_fraction("periods"), 1.0);
    // Learning works with the out-of-tree pieces too.
    let observed = ens.member_mask(0, &[CellType::new("A")]);
    let report = ens.assimilate(&observed, &[CellType::new("A")]).unwrap();
    assert_eq!(report.scores[0], 1.0);
    assert_eq!(ens.generation(), 1);
    assert!(
        ens.members()
            .iter()
            .all(|m| m.state.get("periods") == Some(2.0)),
        "state carries over"
    );
    // The config round-trips with the driver in it.
    let back: CellaConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    assert!(back.ensemble().unwrap().driver.is_some());
    assert_eq!(
        back.ensemble()
            .unwrap()
            .driver
            .as_ref()
            .unwrap()
            .period_steps(),
        Some(3)
    );
}

#[test]
fn the_same_config_evolves_and_the_result_can_be_applied() {
    let json = config_json(
        r#""evolve": {
            "population": 6, "generations": 2, "steps": 8, "repeats": 1,
            "genes": [{"key": "boost", "range": [0.0, 2.0]}],
            "objective": {"metric": "fraction", "types": ["A"], "goal": "maximise"},
            "driver": {"explore_test_driver": {}},
            "forcing": {"level": 0.8}
        }"#,
    );
    let cfg: CellaConfig = serde_json::from_str(&json).unwrap();
    let mut evo = cfg
        .build_evolution()
        .expect("block present")
        .expect("block valid");
    let reports = evo.run(2, |_| {});
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|r| r.invalid == 0 && r.best.is_finite()));
    let mut sim = cfg.build_sim().unwrap();
    evo.apply_best(&mut sim).unwrap();
    // `boost` is a free gene: applying the genome changes no knob, the driver would.
    assert_eq!(sim.get_param("model.rate"), Some(ParamValue::Float(0.5)));
    assert!(cfg.build_ensemble().is_none(), "no ensemble block here");
}

#[test]
fn a_1d_config_takes_both_blocks_and_bad_blocks_are_reported_not_panicked() {
    let init: Vec<&str> = (0..16)
        .map(|i| if i == 8 { "X" } else { "Inactive" })
        .collect();
    let json = format!(
        r#"{{
            "dim": "1d", "width": 16, "history_limit": 0, "seed": 9,
            "initial": {init},
            "rule": {{"subrules": [
                {{"wolfram_code": "30", "randomness": 0.1, "n": 1, "output_type": "X", "current_type": "X", "criteria_type": "X"}},
                {{"wolfram_code": "30", "randomness": null, "n": 1, "output_type": "X", "current_type": "Inactive", "criteria_type": "X"}}
            ]}},
            "ensemble": {{"members": 4, "genes": [{{"key": "rule.subrules[0].randomness", "range": [0.0, 0.5]}}]}},
            "evolve": {{"population": 4, "generations": 1, "steps": 4, "repeats": 1,
                        "genes": [{{"key": "rule.subrules[*].wolfram_code"}}],
                        "objective": {{"metric": "activity", "when": "mean"}}}}
        }}"#,
        init = serde_json::to_string(&init).unwrap()
    );
    let cfg: CellaConfig = serde_json::from_str(&json).unwrap();
    let sim = cfg.build_sim().unwrap();
    assert_eq!(sim.seed(), 9, "the top-level seed reaches the grid");
    let mut ens = cfg.build_ensemble().unwrap().unwrap();
    ens.step_n(3).unwrap();
    assert_eq!(
        ens.track(),
        &[CellType::new("X")],
        "default track: every type but the background"
    );
    let mut evo = cfg.build_evolution().unwrap().unwrap();
    let r = evo.step_generation();
    assert!(r.best.is_finite());
    assert_eq!(r.best_named.len(), 1);

    // A gene naming a knob that does not exist is an error, not a panic.
    let bad = config_json(r#""ensemble": {"members": 2, "genes": [{"key": "model.nope"}]}"#);
    let cfg: CellaConfig = serde_json::from_str(&bad).unwrap();
    let err = cfg.build_ensemble().unwrap().unwrap_err();
    assert!(format!("{err}").contains("model.nope"), "{err}");
    // A free gene without a driver, and the old prior shape, are refused too.
    let bad = config_json(r#""ensemble": {"members": 2, "genes": [{"key": "boost"}]}"#);
    let cfg: CellaConfig = serde_json::from_str(&bad).unwrap();
    assert!(cfg.build_ensemble().unwrap().is_err());
    let old = config_json(r#""ensemble": {"members": 2, "prior": {"p0": [0.1, 0.5]}}"#);
    assert!(
        serde_json::from_str::<CellaConfig>(&old).is_err(),
        "the September 2026 prior block is rejected"
    );
    // No block: None, not an error.
    let none: CellaConfig = serde_json::from_str(&config_json(r#""colors": {}"#)).unwrap();
    assert!(none.build_ensemble().is_none() && none.build_evolution().is_none());
    let _ = Metric::Activity;
}

/// A driver whose `apply` counts its own calls into the member state and
/// makes the model faster each time, so a re-application is observable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct RampDriver {
    period: u64,
}

#[typetag::serde(name = "explore_test_ramp_driver")]
impl MemberDriver for RampDriver {
    fn owned_keys(&self) -> Vec<String> {
        vec!["model.rate".into()]
    }

    fn period_steps(&self) -> Option<u64> {
        (self.period > 0).then_some(self.period)
    }

    fn apply(
        &self,
        sim: &mut Sim,
        _genome: &Genome,
        _space: &GeneSpace,
        _forcing: &Forcing,
        state: &mut MemberState,
    ) -> Result<(), ModelError> {
        let n = state.get("applies").unwrap_or(0.0) + 1.0;
        state.set("applies", n);
        // First application: frozen. Every later one: everything advances.
        let rate = if n >= 2.0 { 1.0 } else { 0.0 };
        sim.set_param("model.rate", ParamValue::Float(rate))
    }

    fn boxed_clone(&self) -> Box<dyn MemberDriver> {
        Box::new(self.clone())
    }
}

#[test]
fn drivers_are_reapplied_at_every_period_boundary_in_both_engines() {
    // Ensemble: after two periods the driver has been applied three times
    // (construction + one per boundary) and the model runs.
    let json = config_json(
        r#""ensemble": {
            "members": 2, "seed": 1, "genes": [], "track": ["A"],
            "driver": {"explore_test_ramp_driver": {"period": 3}}
        }"#,
    );
    let cfg: CellaConfig = serde_json::from_str(&json).unwrap();
    let mut ens = cfg.build_ensemble().unwrap().unwrap();
    let start = ens.state_probability(&[CellType::new("A")]);
    ens.step_n(3).unwrap();
    let applies = |e: &cella_lib::Ensemble| {
        e.members()
            .iter()
            .map(|m| m.state.get("applies").unwrap_or(0.0))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        applies(&ens),
        vec![2.0, 2.0],
        "construction + first boundary"
    );
    assert_eq!(
        ens.state_probability(&[CellType::new("A")]),
        start,
        "frozen for the first period"
    );
    // Two more steps, not three: the test model cycles A -> B -> C -> A, so a
    // full period would land back on the start state.
    ens.step_n(2).unwrap();
    assert_eq!(applies(&ens), vec![2.0, 2.0]);
    assert_ne!(
        ens.state_probability(&[CellType::new("A")]),
        start,
        "moving after the re-application"
    );

    // Evolution: the same genome scores differently with and without period
    // boundaries, because only the boundary re-applies the driver.
    let block = |period: u64| {
        format!(
            r#""evolve": {{
                "population": 4, "elite": 1, "generations": 1, "repeats": 1, "steps": 5, "seed": 1,
                "genes": [{{"key": "model.rate"}}],
                "objective": {{"metric": "activity", "goal": "maximise"}},
                "driver": {{"explore_test_ramp_driver": {{"period": {period}}}}}
            }}"#
        )
    };
    let with: CellaConfig = serde_json::from_str(&config_json(&block(3))).unwrap();
    let without: CellaConfig = serde_json::from_str(&config_json(&block(0))).unwrap();
    let evo_with = with.build_evolution().unwrap().unwrap();
    let evo_without = without.build_evolution().unwrap().unwrap();
    let genome = evo_with.population()[0].genome.clone();
    let frozen = evo_without.evaluate(&genome, 0, 0);
    let moving = evo_with.evaluate(&genome, 0, 0);
    assert!(frozen.is_finite() && moving.is_finite());
    assert_ne!(
        frozen, moving,
        "re-application at the boundary changes the run"
    );
}
