//! The gene list a run searches over (the E25 prior plus whichever knobs
//! extend or trim it) and the config-time overrides (`SMC_SPREAD`,
//! `SMC_WIND_LAW`, `SMC_C2`, `SMC_ARRIVAL_JITTER`, `SMC_SPOT`) that have to
//! land on the `CellaConfig` before it becomes a `Sim`. See the top-level
//! module doc comment in `main.rs` for what every knob does.

use cella_lib::config::CellaConfig;
use cella_lib::wildfire::driver::{
    GENE_CONTAIN_A, GENE_CONTAIN_B, GENE_TAU_DAYS, GENE_WIND_ROT_DEG, GENE_WIND_SCALE,
};
use cella_lib::wildfire::{SpottingParams, WildfireModel};
use cella_lib::GeneSpec;

/// The E25 prior as genes.
pub(crate) fn default_genes() -> Vec<GeneSpec> {
    vec![
        GeneSpec::log_range("model.p0", 0.08, 0.6),
        GeneSpec::range("model.burn_duration", 5.0, 20.0),
        GeneSpec::log_range(GENE_TAU_DAYS, 2.0, 100.0),
        GeneSpec::range(GENE_WIND_SCALE, 0.0, 1.5),
    ]
}

/// Spotting genes (validation E43), added only under `SMC_SPOT=1`. The
/// model key is `spotting.p_spot` / `spotting.median_distance` (see
/// [`enable_spotting`]); the `model.` prefix routes a gene at the attached
/// model the way `model.p0` already does (see the module's gene-key docs).
const GENE_SPOT_P: &str = "model.spotting.p_spot";
const GENE_SPOT_DIST: &str = "model.spotting.median_distance";

/// `p_spot` is a per-step probability (struct doc: "chance per step that a
/// burning cell launches a firebrand"), so it spans orders of magnitude and
/// is drawn log-uniform, like the other probability-shaped genes here
/// (`model.p0`). The range is E7's pre-registered spotting space verbatim
/// (`exp_spotting.py`'s SPOT_GRID: lo 0.001, mid 0.005, far 0.002) — already
/// shown to move burned area 2-4x on these fires, so it is wide enough to
/// show whether spotting reaches new archive shapes without extrapolating
/// past what has actually been tested.
const SPOT_P_LO: f64 = 0.001;
const SPOT_P_HI: f64 = 0.005;

/// Median landing distance in cells, linear (not log: this is a spatial
/// scale, not a rate). E7 tested 5-20 cells; the task brief widens the
/// floor to 2 cells (a jump barely ahead of the front) to also cover
/// short-range spotting, still well inside `SpottingParams::median_distance`'s
/// declared bounds of [0.5, 100] cells.
const SPOT_DIST_LO: f64 = 2.0;
const SPOT_DIST_HI: f64 = 20.0;

/// The full gene list for a run: the prior (default or `SMC_PRIOR`), plus
/// the containment genes under `SMC_CONTAIN`, minus `tau_days` under
/// `SMC_TAU_OFF`, plus the spotting genes under `SMC_SPOT`, plus the
/// wind-rotation gene under `SMC_WIND_ROT_GENE`. Split out of `main` so
/// each knob's effect on the gene list is unit-testable without an env var
/// or a scenario directory.
///
/// `wind_rot_gene` is the gene's half-width in degrees (E30b:
/// `SMC_WIND_ROT_GENE`); `None` (the default) leaves `wind_rot_deg` out of
/// the list entirely, so [`cella_lib::wildfire::driver::WildfireDriver::apply`]
/// never sees a value for it and every member's wind direction is exactly
/// the forcing's own, unchanged (the pre-E30b behaviour).
///
/// `wind_rot_sigma` (Round 7 Task 5: `SMC_WIND_ROT_SIGMA`) is a per-gene
/// mutation-size override for `wind_rot_deg` only, applied to the gene
/// [`wind_rot_gene`] just added — `None` (the default) leaves the gene's
/// `sigma` unset, i.e. it mutates at the engine's own sigma exactly as
/// before this knob existed (byte-identical reports). `Some(0.0)` (E45's
/// Arm B-σ0) freezes the gene: each member keeps the rotation it was born
/// with for the rest of the run, so the ensemble's per-member angular
/// *diversity* survives while the *learning* half of the gene (mutating
/// toward a better bearing) is switched off — see
/// [`cella_lib::explore::genome::GeneSpace::resolve`]'s per-gene `sigma`,
/// which accepts `0.0` for exactly this. Has no effect when
/// `wind_rot_gene` is `None` (nothing to attach a sigma override to).
pub(crate) fn build_genes(
    prior: Vec<GeneSpec>,
    contain: bool,
    tau_off: bool,
    spot: bool,
    wind_rot_gene: Option<f64>,
    wind_rot_sigma: Option<f64>,
) -> Vec<GeneSpec> {
    if wind_rot_sigma.is_some() && wind_rot_gene.is_none() {
        eprintln!(
            "warning: SMC_WIND_ROT_SIGMA is set but SMC_WIND_ROT_GENE is not -- there is no \
             wind_rot_deg gene in this run's list to attach a sigma override to, so \
             SMC_WIND_ROT_SIGMA has no effect"
        );
    }
    let mut genes = prior;
    if contain {
        genes.push(GeneSpec::new(GENE_CONTAIN_A));
        genes.push(GeneSpec::new(GENE_CONTAIN_B));
    }
    if tau_off {
        genes.retain(|g| g.key != GENE_TAU_DAYS);
    }
    if spot {
        genes.push(GeneSpec::log_range(GENE_SPOT_P, SPOT_P_LO, SPOT_P_HI));
        genes.push(GeneSpec::range(GENE_SPOT_DIST, SPOT_DIST_LO, SPOT_DIST_HI));
    }
    if let Some(h) = wind_rot_gene {
        genes.push(GeneSpec {
            sigma: wind_rot_sigma,
            ..GeneSpec::range(GENE_WIND_ROT_DEG, -h, h)
        });
    }
    genes
}

/// Switch spotting on in the config's wildfire model (`SMC_SPOT=1`, E43),
/// so the `model.spotting.*` genes [`build_genes`] adds have something to
/// write into: [`cella_lib::Grid2D::set_model_param`] and the gene machinery
/// both refuse a `spotting.*` key while `WildfireParams::spotting` is `None`
/// (see the `set_param` tests in `cella_lib/src/wildfire/mod.rs`), so this
/// must run before the config is turned into a `Sim`.
///
/// `p_spot` and `median_distance` are genes and are overwritten per genome;
/// `sigma` and `angle_jitter_deg` stay fixed at E7's low setting.
pub(crate) fn enable_spotting(cfg: &mut CellaConfig) {
    let CellaConfig::D2(c) = cfg else {
        panic!("SMC_SPOT needs a 2D scenario config");
    };
    let model = c
        .model
        .as_deref_mut()
        .expect("SMC_SPOT needs a config with a model attached")
        .as_any_mut()
        .downcast_mut::<WildfireModel>()
        .expect("SMC_SPOT needs the wildfire model");
    model.params.spotting = Some(SpottingParams {
        p_spot: SPOT_P_LO,
        median_distance: SPOT_DIST_LO,
        sigma: 0.5,
        angle_jitter_deg: 15.0,
    });
}

/// Override the config's wildfire spread rule / wind law / c2 / jitter
/// (validation E30/E30a: `SMC_SPREAD`, `SMC_WIND_LAW`, `SMC_C2`,
/// `SMC_ARRIVAL_JITTER`). Each of the four is independent: `None` leaves
/// the scenario config's own value untouched, so calling this with all
/// four `None` (every knob unset) is a complete no-op — it does not even
/// look at the config, let alone panic on one without a wildfire model
/// attached. Mirrors [`enable_spotting`]'s downcast-to-`WildfireModel`
/// pattern for the cases where there is something to change.
pub(crate) fn configure_spread(
    cfg: &mut CellaConfig,
    spread: Option<&str>,
    wind_law: Option<&str>,
    c2: Option<f64>,
    arrival_jitter: Option<f64>,
) {
    if spread.is_none() && wind_law.is_none() && c2.is_none() && arrival_jitter.is_none() {
        return;
    }
    let CellaConfig::D2(c) = cfg else {
        panic!("SMC_SPREAD/SMC_WIND_LAW/SMC_C2/SMC_ARRIVAL_JITTER need a 2D scenario config");
    };
    let model = c
        .model
        .as_deref_mut()
        .expect(
            "SMC_SPREAD/SMC_WIND_LAW/SMC_C2/SMC_ARRIVAL_JITTER need a config with a model attached",
        )
        .as_any_mut()
        .downcast_mut::<WildfireModel>()
        .expect("SMC_SPREAD/SMC_WIND_LAW/SMC_C2/SMC_ARRIVAL_JITTER need the wildfire model");
    if let Some(v) = spread {
        model.params.spread = v.to_string();
    }
    if let Some(v) = wind_law {
        model.params.wind_law = v.to_string();
    }
    if let Some(v) = c2 {
        model.params.c2 = v;
    }
    if let Some(v) = arrival_jitter {
        model.params.arrival_jitter = v;
    }
}

/// E43: `SMC_SPOT=1` must add the spotting genes to the gene list and
/// switch spotting on in the config the runner builds. See `build_genes`
/// and `enable_spotting`.
#[cfg(test)]
mod spot_gene_tests {
    use super::*;
    use cella_lib::config::Config2D;
    use cella_lib::explore::Scale;
    use cella_lib::wildfire::{FuelClass, WildfireEnv, WildfireParams};
    use cella_lib::{ParamValue, Rule2D};

    fn tiny_wildfire_params() -> WildfireParams {
        WildfireParams {
            seed: 0,
            p0: 0.3,
            fuels: vec![FuelClass {
                name: "Forest".into(),
                veg_factor: 1.0,
            }],
            wind_speed: 0.0,
            wind_from_deg: 0.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 5,
            spotting: None,
            burning_name: None,
            burned_name: None,
            spread: "bernoulli".into(),
            arrival_jitter: 0.2,
            wind_law: "exponential".into(),
        }
    }

    /// A minimal 2D config with a wildfire model attached and spotting off,
    /// standing in for a scenario's `config.json` after it is loaded.
    fn tiny_config() -> CellaConfig {
        let model = WildfireModel::new(tiny_wildfire_params(), WildfireEnv::default());
        CellaConfig::D2(Config2D {
            width: 2,
            height: 2,
            history_limit: 1,
            initial: vec!["Forest".into(); 4],
            rule: Rule2D { subrules: vec![] },
            model: Some(Box::new(model)),
            ..Config2D::default()
        })
    }

    #[test]
    fn without_smc_spot_the_gene_list_and_config_are_unchanged() {
        let genes = build_genes(default_genes(), false, false, false, None, None);
        assert!(genes.iter().all(|g| g.key != GENE_SPOT_P && g.key != GENE_SPOT_DIST));

        let cfg = tiny_config();
        let CellaConfig::D2(c) = &cfg else {
            unreachable!()
        };
        let model = c.model.as_ref().expect("model attached");
        assert!(
            model.get_param("spotting.p_spot").is_none(),
            "spotting must stay off unless SMC_SPOT=1"
        );
    }

    #[test]
    fn smc_spot_adds_both_spotting_genes_with_the_documented_ranges() {
        let genes = build_genes(default_genes(), false, false, true, None, None);
        let p_spot = genes
            .iter()
            .find(|g| g.key == GENE_SPOT_P)
            .expect("p_spot gene present");
        assert_eq!(p_spot.range, Some([SPOT_P_LO, SPOT_P_HI]));
        assert_eq!(p_spot.scale, Scale::Log, "p_spot is drawn log-uniform");

        let dist = genes
            .iter()
            .find(|g| g.key == GENE_SPOT_DIST)
            .expect("median_distance gene present");
        assert_eq!(dist.range, Some([SPOT_DIST_LO, SPOT_DIST_HI]));
        assert_eq!(dist.scale, Scale::Linear);

        // SMC_SPOT does not disturb the default genes or SMC_CONTAIN/TAU_OFF.
        assert_eq!(genes.len(), default_genes().len() + 2);
    }

    #[test]
    fn smc_spot_combines_with_contain_and_tau_off() {
        let genes = build_genes(default_genes(), true, true, true, None, None);
        let keys: Vec<&str> = genes.iter().map(|g| g.key.as_str()).collect();
        assert!(keys.contains(&GENE_CONTAIN_A));
        assert!(keys.contains(&GENE_CONTAIN_B));
        assert!(keys.contains(&GENE_SPOT_P));
        assert!(keys.contains(&GENE_SPOT_DIST));
        assert!(
            !keys.contains(&GENE_TAU_DAYS),
            "SMC_TAU_OFF still drops tau_days"
        );
    }

    #[test]
    fn enable_spotting_turns_spotting_on_in_the_config() {
        let mut cfg = tiny_config();
        enable_spotting(&mut cfg);
        let CellaConfig::D2(c) = &cfg else {
            unreachable!()
        };
        let model = c.model.as_ref().expect("model attached");
        // get_param only returns Some for spotting.* once spotting is on
        // (see the wildfire model's own set_param/get_param tests).
        assert_eq!(
            model.get_param("spotting.p_spot"),
            Some(ParamValue::Float(SPOT_P_LO))
        );
        assert_eq!(
            model.get_param("spotting.median_distance"),
            Some(ParamValue::Float(SPOT_DIST_LO))
        );
    }
}

/// E30/E30a: `SMC_SPREAD`/`SMC_WIND_LAW`/`SMC_C2`/`SMC_ARRIVAL_JITTER` must
/// set the corresponding param on the config's wildfire model when present,
/// and leave the config's own values untouched when absent. See
/// `configure_spread`.
#[cfg(test)]
mod arrival_config_tests {
    use super::*;
    use cella_lib::config::Config2D;
    use cella_lib::wildfire::{FuelClass, WildfireEnv, WildfireParams};
    use cella_lib::{ParamValue, Rule2D};

    fn tiny_wildfire_params() -> WildfireParams {
        WildfireParams {
            seed: 0,
            p0: 0.3,
            fuels: vec![FuelClass {
                name: "Forest".into(),
                veg_factor: 1.0,
            }],
            wind_speed: 0.0,
            wind_from_deg: 0.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 5,
            spotting: None,
            burning_name: None,
            burned_name: None,
            spread: "bernoulli".into(),
            arrival_jitter: 0.2,
            wind_law: "exponential".into(),
        }
    }

    /// A minimal 2D config with a wildfire model attached, standing in for
    /// a scenario's `config.json` after it is loaded.
    fn tiny_config() -> CellaConfig {
        let model = WildfireModel::new(tiny_wildfire_params(), WildfireEnv::default());
        CellaConfig::D2(Config2D {
            width: 2,
            height: 2,
            history_limit: 1,
            initial: vec!["Forest".into(); 4],
            rule: Rule2D { subrules: vec![] },
            model: Some(Box::new(model)),
            ..Config2D::default()
        })
    }

    fn spread_param(cfg: &CellaConfig, key: &str) -> ParamValue {
        let CellaConfig::D2(c) = cfg else { unreachable!() };
        let model = c.model.as_ref().expect("model attached");
        model.get_param(key).expect("param present")
    }

    #[test]
    fn unset_knobs_leave_the_config_unchanged() {
        let mut cfg = tiny_config();
        configure_spread(&mut cfg, None, None, None, None);
        assert_eq!(
            spread_param(&cfg, "spread"),
            ParamValue::Choice("bernoulli".into())
        );
        assert_eq!(
            spread_param(&cfg, "wind_law"),
            ParamValue::Choice("exponential".into())
        );
        assert_eq!(spread_param(&cfg, "c2"), ParamValue::Float(0.131));
        assert_eq!(spread_param(&cfg, "arrival_jitter"), ParamValue::Float(0.2));
    }

    #[test]
    fn smc_spread_and_smc_wind_law_set_e30s_recommended_kernel() {
        let mut cfg = tiny_config();
        configure_spread(&mut cfg, Some("arrival"), Some("rear_focus"), None, None);
        assert_eq!(
            spread_param(&cfg, "spread"),
            ParamValue::Choice("arrival".into())
        );
        assert_eq!(
            spread_param(&cfg, "wind_law"),
            ParamValue::Choice("rear_focus".into())
        );
        // c2/arrival_jitter were not passed, so they stay at the config's
        // own defaults even though the other two knobs changed.
        assert_eq!(spread_param(&cfg, "c2"), ParamValue::Float(0.131));
        assert_eq!(spread_param(&cfg, "arrival_jitter"), ParamValue::Float(0.2));
    }

    #[test]
    fn smc_c2_and_smc_arrival_jitter_set_their_own_params_independently() {
        let mut cfg = tiny_config();
        configure_spread(&mut cfg, None, None, Some(0.45), Some(0.5));
        // spread/wind_law untouched.
        assert_eq!(
            spread_param(&cfg, "spread"),
            ParamValue::Choice("bernoulli".into())
        );
        assert_eq!(
            spread_param(&cfg, "wind_law"),
            ParamValue::Choice("exponential".into())
        );
        assert_eq!(spread_param(&cfg, "c2"), ParamValue::Float(0.45));
        assert_eq!(spread_param(&cfg, "arrival_jitter"), ParamValue::Float(0.5));
    }
}

/// E30b: `SMC_WIND_ROT_GENE` must add `wind_rot_deg` to the gene list only
/// when set, at the requested half-width. See `build_genes`. The matching
/// `SMC_STEPS_SCALE` tests live in `knobs.rs` beside `apply_steps_scale`,
/// which is the function that knob actually changes.
#[cfg(test)]
mod e30b_wind_rot_gene_tests {
    use super::*;
    use cella_lib::explore::Scale;

    #[test]
    fn without_smc_wind_rot_gene_the_gene_list_is_unchanged() {
        let genes = build_genes(default_genes(), true, true, false, None, None);
        assert!(
            genes.iter().all(|g| g.key != GENE_WIND_ROT_DEG),
            "wind_rot_deg must stay out of the list unless SMC_WIND_ROT_GENE is set"
        );
    }

    #[test]
    fn smc_wind_rot_gene_adds_the_gene_at_the_requested_half_width() {
        let genes = build_genes(default_genes(), true, true, false, Some(90.0), None);
        let g = genes
            .iter()
            .find(|g| g.key == GENE_WIND_ROT_DEG)
            .expect("wind_rot_deg gene present");
        assert_eq!(g.range, Some([-90.0, 90.0]));
        assert_eq!(g.scale, Scale::Linear, "an angle, not a rate: linear, not log");
        // It combines with the other knobs rather than replacing them.
        let keys: Vec<&str> = genes.iter().map(|g| g.key.as_str()).collect();
        assert!(keys.contains(&GENE_CONTAIN_A));
        assert!(keys.contains(&GENE_CONTAIN_B));
        assert!(!keys.contains(&GENE_TAU_DAYS), "SMC_TAU_OFF still drops tau_days");
        assert_eq!(genes.len(), default_genes().len() - 1 + 2 + 1);
    }

    #[test]
    fn a_different_half_width_is_reflected_in_the_range() {
        let genes = build_genes(default_genes(), false, false, false, Some(15.0), None);
        let g = genes
            .iter()
            .find(|g| g.key == GENE_WIND_ROT_DEG)
            .expect("wind_rot_deg gene present");
        assert_eq!(g.range, Some([-15.0, 15.0]));
    }

    /// Round 7 Task 5 (E45): `SMC_WIND_ROT_SIGMA` unset (`None`) must leave
    /// the `wind_rot_deg` gene spec's `sigma` field unset too -- the Task 1
    /// acceptance property (unset ⇒ byte-identical reports) depends on this,
    /// since `GeneSpec.sigma` is what would otherwise get serialised into
    /// `genes` in the report.
    #[test]
    fn smc_wind_rot_sigma_unset_leaves_the_gene_spec_unchanged() {
        let genes = build_genes(default_genes(), true, true, false, Some(90.0), None);
        let g = genes
            .iter()
            .find(|g| g.key == GENE_WIND_ROT_DEG)
            .expect("wind_rot_deg gene present");
        assert_eq!(g.sigma, None, "unset SMC_WIND_ROT_SIGMA must not set a sigma override");
    }

    /// `SMC_WIND_ROT_SIGMA=0` (Arm B-σ0) must land as a per-gene `sigma`
    /// override on `wind_rot_deg` only -- every other gene in the same
    /// list (e.g. `model.p0`) must keep its own `sigma` unset, since the
    /// override is Task 5's per-gene mechanism, not a change to the
    /// engine's own default sigma.
    #[test]
    fn smc_wind_rot_sigma_sets_a_per_gene_override_on_wind_rot_deg_only() {
        let genes = build_genes(default_genes(), false, false, false, Some(90.0), Some(0.0));
        let rot = genes
            .iter()
            .find(|g| g.key == GENE_WIND_ROT_DEG)
            .expect("wind_rot_deg gene present");
        assert_eq!(rot.sigma, Some(0.0));
        assert_eq!(rot.range, Some([-90.0, 90.0]), "sigma does not disturb the range");
        for other in genes.iter().filter(|g| g.key != GENE_WIND_ROT_DEG) {
            assert_eq!(
                other.sigma, None,
                "SMC_WIND_ROT_SIGMA must not touch any gene but wind_rot_deg ({})",
                other.key
            );
        }
    }

    /// `SMC_WIND_ROT_SIGMA` set without `SMC_WIND_ROT_GENE` has nothing to
    /// attach a sigma override to -- the gene stays out of the list
    /// entirely, same as with both knobs unset.
    #[test]
    fn smc_wind_rot_sigma_has_no_effect_when_the_gene_is_absent() {
        let genes = build_genes(default_genes(), false, false, false, None, Some(0.0));
        assert!(
            genes.iter().all(|g| g.key != GENE_WIND_ROT_DEG),
            "wind_rot_deg must stay out of the list when SMC_WIND_ROT_GENE is unset, \
             regardless of SMC_WIND_ROT_SIGMA"
        );
    }
}
