//! [`WildfireDriver`]: the worked example of a [`MemberDriver`].
//!
//! Read this file when you write a driver for your own model. It does the
//! three things a driver exists for:
//!
//! 1. **Turn forcing into model settings.** Each window the caller supplies
//!    `hours` since ignition and the wind (`wind_speed_ms`, `wind_from_deg`).
//!    The driver writes the wind into the model, multiplied by the member's
//!    `wind_scale` gene (how much this member trusts the weather station),
//!    and sets `p0` to the member's base value times an optional decay
//!    `exp(-hours / (24 · tau_days))` — the "fires slow down as the days pass"
//!    effect of validation experiment E16. When the caller gives no forcing
//!    (the GUI, say), hours come from the step count and `steps_per_day`, and
//!    the wind from an optional `weather` schedule or the model's own
//!    setting.
//! 2. **Own a knob it can write better.** `p0` is baked into the model's
//!    per-cell table, so the generic setter would rebuild everything; the
//!    driver claims `model.p0` and uses the exact, cheap
//!    [`WildfireModel::set_p0`] instead, folding the decay in at the same time.
//! 3. **Stop members with a published rule.** Once a simulated day (every
//!    `steps_per_day` steps) a still-burning member is *contained* with
//!    probability `1 / (1 + exp(-(a + b · ln g)))`, where `g` is how much it
//!    grew that day — the containment model FSim uses (Finney et al. 2011),
//!    validated here as E28. `a` and `b` are the genes `contain_a` and
//!    `contain_b`; leave them out and no member is ever contained. A contained
//!    member keeps its state but its `p0` becomes 0 for good.
//!
//! Everything the driver remembers per member sits in the [`MemberState`]
//! under plain names (`p0_base`, `contained`, `burned_at_day_start`), so the
//! ensemble can report `state_fraction("contained")` without knowing fire.

use serde::{Deserialize, Serialize};

use super::WildfireModel;
use crate::explore::driver::{Forcing, MemberDriver, MemberState};
use crate::explore::genome::{Gene, GeneSpace, Genome};
use crate::explore::sim::Sim;
use crate::external::ModelError;
use crate::rng::Rng;

/// Free gene: multiplier on the wind speed the forcing supplies.
pub const GENE_WIND_SCALE: &str = "wind_scale";
/// Free gene: decay time-scale in days for `p0`; absent means no decay.
pub const GENE_TAU_DAYS: &str = "tau_days";
/// Free gene: intercept of the daily containment probability.
pub const GENE_CONTAIN_A: &str = "contain_a";
/// Free gene: slope on `ln(growth)` of the daily containment probability.
pub const GENE_CONTAIN_B: &str = "contain_b";
/// The knob this driver writes itself.
pub const OWNED_P0: &str = "model.p0";

/// Forcing key: hours since ignition at the start of the coming steps.
pub const FORCING_HOURS: &str = "hours";
/// Forcing key: wind speed in m/s.
pub const FORCING_WIND_SPEED: &str = "wind_speed_ms";
/// Forcing key: compass bearing the wind blows *from*, degrees.
pub const FORCING_WIND_FROM: &str = "wind_from_deg";

/// Member state: the p0 the member started with, before any decay.
pub const STATE_P0_BASE: &str = "p0_base";
/// Member state flag: this member has been contained.
pub const STATE_CONTAINED: &str = "contained";
/// Member state: burned cells at the last period boundary.
pub const STATE_BURNED_AT_DAY_START: &str = "burned_at_day_start";

/// One entry of a wind schedule: from `hours` on, this wind applies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherWindow {
    pub hours: f64,
    pub speed_ms: f64,
    pub from_deg: f64,
}

/// Wildfire-specific behaviour for ensemble members. See the module docs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WildfireDriver {
    /// Steps per simulated day; sets the containment period and converts
    /// step counts to hours when no `hours` forcing is given.
    #[serde(default = "default_steps_per_day")]
    pub steps_per_day: u64,
    /// Optional wind schedule used when no wind forcing is given. Empty
    /// keeps the model's own constant wind.
    #[serde(default)]
    pub weather: Vec<WeatherWindow>,
}

fn default_steps_per_day() -> u64 {
    50
}

impl Default for WildfireDriver {
    fn default() -> Self {
        WildfireDriver {
            steps_per_day: default_steps_per_day(),
            weather: Vec::new(),
        }
    }
}

fn model_of(sim: &mut Sim) -> Result<&mut WildfireModel, ModelError> {
    sim.model_mut()
        .and_then(|m| m.as_any_mut().downcast_mut::<WildfireModel>())
        .ok_or_else(|| {
            ModelError::InvalidParam(
                "the wildfire driver needs a grid with the wildfire model attached".into(),
            )
        })
}

impl WildfireDriver {
    /// The wind in force at `hours` from the schedule, if there is one.
    fn scheduled_wind(&self, hours: f64) -> Option<(f64, f64)> {
        let mut pick: Option<&WeatherWindow> = None;
        for w in &self.weather {
            if w.hours <= hours && pick.is_none_or(|p| w.hours >= p.hours) {
                pick = Some(w);
            }
        }
        pick.or(self.weather.first())
            .map(|w| (w.speed_ms, w.from_deg))
    }
}

#[typetag::serde(name = "wildfire")]
impl MemberDriver for WildfireDriver {
    fn free_genes(&self) -> Vec<Gene> {
        vec![
            Gene::float(GENE_WIND_SCALE, 0.0, 1.5, false),
            Gene::float(GENE_TAU_DAYS, 2.0, 100.0, true),
            Gene::float(GENE_CONTAIN_A, -6.0, -1.0, false),
            Gene::float(GENE_CONTAIN_B, -2.0, -0.3, false),
        ]
    }

    fn owned_keys(&self) -> Vec<String> {
        vec![OWNED_P0.to_string()]
    }

    fn period_steps(&self) -> Option<u64> {
        Some(self.steps_per_day)
    }

    fn apply(
        &self,
        sim: &mut Sim,
        genome: &Genome,
        space: &GeneSpace,
        forcing: &Forcing,
        state: &mut MemberState,
    ) -> Result<(), ModelError> {
        let step = sim.step_count();
        let model = model_of(sim)?;
        let hours = forcing
            .get(FORCING_HOURS)
            .copied()
            .unwrap_or_else(|| step as f64 / self.steps_per_day.max(1) as f64 * 24.0);
        let forced_wind = match (
            forcing.get(FORCING_WIND_SPEED),
            forcing.get(FORCING_WIND_FROM),
        ) {
            (Some(s), Some(f)) => Some((*s, *f)),
            (Some(s), None) => Some((*s, model.params.wind_from_deg)),
            (None, Some(f)) => Some((model.params.wind_speed, *f)),
            (None, None) => None,
        };
        let (speed, from) = forced_wind
            .or_else(|| self.scheduled_wind(hours))
            .unwrap_or((model.params.wind_speed, model.params.wind_from_deg));
        let wind_scale = space.float(genome, GENE_WIND_SCALE).unwrap_or(1.0);
        model.params.wind_speed = speed * wind_scale;
        model.params.wind_from_deg = from.rem_euclid(360.0);

        // The base p0 comes from the gene when there is one (a child's mutated
        // gene must win over the scratch it inherited from its parent). With
        // no gene it is captured from the model once: set_p0 overwrites
        // params.p0, so reading it back later would compound the decay.
        let p0_base = match space.float(genome, OWNED_P0) {
            Some(v) => {
                state.set(STATE_P0_BASE, v);
                v
            }
            None => match state.get(STATE_P0_BASE) {
                Some(v) => v,
                None => {
                    let v = model.params.p0;
                    state.set(STATE_P0_BASE, v);
                    v
                }
            },
        };
        let decay = match space.float(genome, GENE_TAU_DAYS) {
            Some(tau) if tau > 0.0 => (-hours / (24.0 * tau)).exp(),
            _ => 1.0,
        };
        let p0 = if state.flag(STATE_CONTAINED) {
            0.0
        } else {
            (p0_base * decay).clamp(0.0, 1.0)
        };
        model.set_p0(p0)
    }

    fn period_end(
        &self,
        sim: &mut Sim,
        genome: &Genome,
        space: &GeneSpace,
        state: &mut MemberState,
        rng: &mut Rng,
    ) -> Result<(), ModelError> {
        let (Some(a), Some(b)) = (
            space.float(genome, GENE_CONTAIN_A),
            space.float(genome, GENE_CONTAIN_B),
        ) else {
            return Ok(());
        };
        let burned = {
            let model = model_of(sim)?;
            let types = [model.burning_type(), model.burned_type()];
            sim.mask(&types).iter().filter(|x| **x).count()
        };
        let before = state.get(STATE_BURNED_AT_DAY_START).unwrap_or(0.0);
        if before > 0.0 && !state.flag(STATE_CONTAINED) {
            let growth = ((burned as f64 - before) / before).max(1e-4);
            let logit = a + b * growth.ln();
            let p = 1.0 / (1.0 + (-logit).exp());
            if rng.uniform() < p {
                state.set(STATE_CONTAINED, 1.0);
                model_of(sim)?.set_p0(0.0)?;
            }
        }
        state.set(STATE_BURNED_AT_DAY_START, burned as f64);
        Ok(())
    }

    fn boxed_clone(&self) -> Box<dyn MemberDriver> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explore::ensemble::{Ensemble, EnsembleConfig};
    use crate::explore::genome::GeneSpec;
    use crate::grid2d::Grid2D;
    use crate::rules::Rule2D;
    use crate::types::CellType;
    use crate::wildfire::{FuelClass, WildfireEnv, WildfireParams};

    fn template(w: usize, h: usize) -> Sim {
        let forest = CellType::new("Forest");
        let mut cells = vec![forest; w * h];
        cells[(h / 2) * w + w / 2] = CellType::new("Burning");
        let params = WildfireParams {
            seed: 0,
            p0: 0.3,
            fuels: vec![FuelClass {
                name: "Forest".into(),
                veg_factor: 1.0,
            }],
            wind_speed: 0.0,
            wind_from_deg: 270.0,
            c1: 0.045,
            c2: 0.131,
            slope_a: 0.078,
            cell_size: 30.0,
            burn_duration: 5,
            spotting: None,
            burning_name: None,
            burned_name: None,
        };
        let mut g = Grid2D::new(w, h, 0, cells, Rule2D { subrules: vec![] });
        g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
            .unwrap();
        Sim::D2(g)
    }

    fn fire_genes() -> Vec<GeneSpec> {
        vec![
            GeneSpec::log_range("model.p0", 0.08, 0.6),
            GeneSpec::range("model.burn_duration", 5.0, 20.0),
            GeneSpec::new(GENE_TAU_DAYS),
            GeneSpec::new(GENE_WIND_SCALE),
        ]
    }

    fn fire_config(members: usize, driver: WildfireDriver) -> EnsembleConfig {
        EnsembleConfig {
            members,
            genes: fire_genes(),
            track: vec!["Burning".into(), "BurnedOut".into()],
            driver: Some(Box::new(driver)),
            ..EnsembleConfig::default()
        }
    }

    fn burning() -> [CellType; 2] {
        [CellType::new("Burning"), CellType::new("BurnedOut")]
    }

    fn forcing(hours: f64, speed: f64, from: f64) -> Forcing {
        let mut f = Forcing::new();
        f.insert(FORCING_HOURS.into(), hours);
        f.insert(FORCING_WIND_SPEED.into(), speed);
        f.insert(FORCING_WIND_FROM.into(), from);
        f
    }

    fn model_params(sim: &mut Sim) -> WildfireParams {
        model_of(sim).unwrap().params.clone()
    }

    #[test]
    fn genes_are_written_into_every_member_through_the_driver() {
        let mut e =
            Ensemble::new(template(12, 12), &fire_config(8, WildfireDriver::default())).unwrap();
        e.set_forcing(forcing(0.0, 4.0, 90.0)).unwrap();
        let space = e.space().clone();
        for m in e.members_mut() {
            let genome = m.genome.clone();
            let p = model_params(&mut m.sim);
            let p0 = space.float(&genome, "model.p0").unwrap();
            assert!(
                (p.p0 - p0).abs() < 1e-12,
                "no decay at hour 0: p0 is the gene"
            );
            assert_eq!(
                p.burn_duration as f64,
                space.float(&genome, "model.burn_duration").unwrap()
            );
            let ws = space.float(&genome, GENE_WIND_SCALE).unwrap();
            assert!((p.wind_speed - 4.0 * ws).abs() < 1e-12);
            assert_eq!(p.wind_from_deg, 90.0);
            assert_eq!(p.seed, m.seed, "each member has its own seed");
            assert_eq!(m.state.get(STATE_P0_BASE), Some(p0));
        }
        // A day later the decay has bitten, from the captured base, not compounding.
        e.set_forcing(forcing(24.0, 4.0, 90.0)).unwrap();
        e.set_forcing(forcing(24.0, 4.0, 90.0)).unwrap();
        for m in e.members_mut() {
            let genome = m.genome.clone();
            let tau = space.float(&genome, GENE_TAU_DAYS).unwrap();
            let expect = space.float(&genome, "model.p0").unwrap() * (-1.0 / tau).exp();
            assert!((model_params(&mut m.sim).p0 - expect).abs() < 1e-9);
        }
    }

    #[test]
    fn probability_map_grows_with_steps_and_members_disagree() {
        let mut e =
            Ensemble::new(template(16, 16), &fire_config(8, WildfireDriver::default())).unwrap();
        let types = burning();
        let p0 = e.state_probability(&types);
        assert_eq!(
            p0.iter().filter(|&&p| p == 1.0).count(),
            1,
            "only the ignition, in every member"
        );
        e.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
        e.step_n(6).unwrap();
        let p1 = e.state_probability(&types);
        assert!(
            p1.iter().sum::<f32>() > p0.iter().sum::<f32>(),
            "fire spread in some members"
        );
        assert!(
            p1.iter().any(|&p| p > 0.0 && p < 1.0),
            "members with different p0 disagree somewhere"
        );
        assert!(
            e.consensus(&types, 0.5)[8 * 16 + 8],
            "the ignition is in every member's burned set"
        );
    }

    #[test]
    fn assimilation_keeps_fire_state_and_learns() {
        let cfg = EnsembleConfig {
            beta: 30.0,
            immigrants: 0.25,
            ..fire_config(8, WildfireDriver::default())
        };
        let mut e = Ensemble::new(template(16, 16), &cfg).unwrap();
        let types = burning();
        e.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
        e.step_n(8).unwrap();
        let observed = e.member_mask(0, &types);
        let rep = e.assimilate(&observed, &types).unwrap();
        assert_eq!(rep.scores[0], 1.0);
        assert_eq!(rep.immigrants, 2);
        assert!(
            rep.parents.iter().filter(|&&p| p == 0).count() >= 4,
            "{:?}",
            rep.parents
        );
        assert!(e.consensus(&types, 0.5)[8 * 16 + 8]);
        assert_eq!(e.step_count(), 8);
        // Children carry their own genome's p0, not the parent's scratch.
        let space = e.space().clone();
        for m in e.members_mut() {
            let g = m.genome.clone();
            let p = model_params(&mut m.sim);
            let gene = space.float(&g, "model.p0").unwrap();
            assert!(
                (p.p0 - gene).abs() < 1e-9,
                "model p0 {} vs gene {gene}",
                p.p0
            );
            assert_eq!(m.state.get(STATE_P0_BASE), Some(gene));
        }
    }

    #[test]
    fn containment_stops_slow_members_and_keeps_them_stopped() {
        // Genes that make containment near-certain for any growth: a = 8, b = 0.
        let mut genes = fire_genes();
        genes.push(GeneSpec::range(GENE_CONTAIN_A, 8.0, 8.0));
        genes.push(GeneSpec::range(GENE_CONTAIN_B, 0.0, 0.0));
        let driver = WildfireDriver {
            steps_per_day: 6,
            weather: vec![],
        };
        let cfg = EnsembleConfig {
            members: 6,
            genes,
            ..fire_config(6, driver)
        };
        let mut e = Ensemble::new(template(16, 16), &cfg).unwrap();
        let types = burning();
        e.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
        // The first period boundary only records the starting size.
        e.step_n(6).unwrap();
        assert_eq!(e.state_fraction(STATE_CONTAINED), 0.0);
        e.step_n(6).unwrap();
        assert!(
            e.state_fraction(STATE_CONTAINED) >= 5.0 / 6.0,
            "{}",
            e.state_fraction(STATE_CONTAINED)
        );
        // Contained members no longer spread, even after the forcing is re-applied.
        let before = e.state_probability(&types);
        e.set_forcing(forcing(24.0, 0.0, 270.0)).unwrap();
        for m in e.members_mut() {
            if m.state.flag(STATE_CONTAINED) {
                assert_eq!(model_params(&mut m.sim).p0, 0.0);
            }
        }
        e.step_n(6).unwrap();
        let after = e.state_probability(&types);
        let grew = after.iter().zip(&before).filter(|(a, b)| a > b).count();
        assert!(
            grew <= 16 * 16 / 4,
            "only the uncontained minority can add cells: {grew}"
        );
        // Containment survives resampling.
        let obs = e.member_mask(0, &types);
        e.assimilate(&obs, &types).unwrap();
        assert!(e.state_fraction(STATE_CONTAINED) > 0.5);
        // Without the containment genes nothing is ever contained.
        let mut plain = Ensemble::new(
            template(8, 8),
            &fire_config(
                4,
                WildfireDriver {
                    steps_per_day: 3,
                    weather: vec![],
                },
            ),
        )
        .unwrap();
        plain.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
        plain.step_n(9).unwrap();
        assert_eq!(plain.state_fraction(STATE_CONTAINED), 0.0);
    }

    #[test]
    fn without_forcing_hours_come_from_steps_and_wind_from_the_schedule() {
        let driver = WildfireDriver {
            steps_per_day: 4,
            weather: vec![
                WeatherWindow {
                    hours: 0.0,
                    speed_ms: 2.0,
                    from_deg: 0.0,
                },
                WeatherWindow {
                    hours: 24.0,
                    speed_ms: 6.0,
                    from_deg: 180.0,
                },
            ],
        };
        assert_eq!(driver.scheduled_wind(0.0), Some((2.0, 0.0)));
        assert_eq!(driver.scheduled_wind(23.9), Some((2.0, 0.0)));
        assert_eq!(driver.scheduled_wind(24.0), Some((6.0, 180.0)));
        assert_eq!(
            driver.scheduled_wind(-5.0),
            Some((2.0, 0.0)),
            "before the schedule: its first entry"
        );
        assert_eq!(WildfireDriver::default().scheduled_wind(3.0), None);
        let genes = vec![
            GeneSpec::range(GENE_WIND_SCALE, 1.0, 1.0),
            GeneSpec::range(GENE_TAU_DAYS, 1.0, 1.0),
        ];
        let cfg = EnsembleConfig {
            members: 1,
            genes,
            track: vec!["Burning".into()],
            driver: Some(Box::new(driver)),
            ..EnsembleConfig::default()
        };
        let mut e = Ensemble::new(template(8, 8), &cfg).unwrap();
        let p = model_params(&mut e.members_mut()[0].sim);
        assert_eq!((p.wind_speed, p.wind_from_deg), (2.0, 0.0));
        assert!(
            (p.p0 - 0.3).abs() < 1e-12,
            "base p0 from the model when there is no gene"
        );
        e.step_n(4).unwrap();
        // Re-applying with the empty forcing derives hours = 24 from the steps.
        e.set_forcing(Forcing::new()).unwrap();
        let p = model_params(&mut e.members_mut()[0].sim);
        assert_eq!((p.wind_speed, p.wind_from_deg), (6.0, 180.0));
        assert!(
            (p.p0 - 0.3 * (-1.0f64).exp()).abs() < 1e-9,
            "one day of decay at tau = 1"
        );
        // A partial forcing keeps the other component from the model.
        let mut half = Forcing::new();
        half.insert(FORCING_WIND_SPEED.into(), 3.0);
        e.set_forcing(half).unwrap();
        let p = model_params(&mut e.members_mut()[0].sim);
        assert_eq!((p.wind_speed, p.wind_from_deg), (3.0, 180.0));
    }

    #[test]
    fn the_driver_refuses_a_grid_without_the_wildfire_model_and_round_trips_json() {
        let alive = CellType::from("Alive");
        let plain = Sim::D2(Grid2D::new(
            4,
            4,
            0,
            vec![alive; 16],
            Rule2D { subrules: vec![] },
        ));
        let cfg = EnsembleConfig {
            members: 2,
            genes: vec![],
            track: vec![],
            driver: Some(Box::new(WildfireDriver::default())),
            ..EnsembleConfig::default()
        };
        let err = Ensemble::new(plain, &cfg).unwrap_err();
        assert!(format!("{err}").contains("wildfire model"), "{err}");
        let json = r#"{"members": 4, "driver": {"wildfire": {"steps_per_day": 12}}, "genes": [{"key": "tau_days"}]}"#;
        let c: EnsembleConfig = serde_json::from_str(json).unwrap();
        let d = c.driver.as_ref().unwrap();
        assert_eq!(d.period_steps(), Some(12));
        assert_eq!(d.owned_keys(), vec![OWNED_P0.to_string()]);
        assert_eq!(d.free_genes().len(), 4);
        let back = serde_json::to_string(&c).unwrap();
        assert!(back.contains(r#""wildfire""#), "{back}");
        let again: EnsembleConfig = serde_json::from_str(&back).unwrap();
        assert_eq!(again.driver.unwrap().period_steps(), Some(12));
        assert!(
            serde_json::from_str::<EnsembleConfig>(r#"{"driver": {"wildfire": {"bogus": 1}}}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<EnsembleConfig>(r#"{"driver": {"nope": {}}}"#).is_err());
        // The free gene needs a driver: the same genes with none are refused.
        let e = Ensemble::new(
            template(6, 6),
            &EnsembleConfig {
                members: 1,
                genes: vec![GeneSpec::new(GENE_TAU_DAYS)],
                ..EnsembleConfig::default()
            },
        );
        assert!(e.is_err());
    }
}
