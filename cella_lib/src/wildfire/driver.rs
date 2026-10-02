//! [`WildfireDriver`]: the worked example of a [`MemberDriver`].
//!
//! A *driver* adapts one ensemble member to a particular model: it turns the
//! outside world's inputs (forcing) into model settings, and applies any
//! rules that act on a whole member. Read this file when you write a driver
//! for your own model. It does the three things a driver exists for (the
//! "E" numbers are validation experiments, written up under
//! `validation/experiments/`):
//!
//! 1. **Turn forcing into model settings.** Each window the caller supplies
//!    `hours` since ignition and the wind (`wind_speed_ms`, `wind_from_deg`).
//!    The driver writes the wind into the model, multiplied by the member's
//!    `wind_scale` gene (how much this member trusts the weather station),
//!    and sets `p0` to the member's base value times an optional decay
//!    `exp(-hours / (24 · tau_days))` — the "fires slow down as the days pass"
//!    effect of validation experiment E16 (decay only applies when the member
//!    has a `tau_days` gene). When the caller gives no forcing
//!    (the GUI, say), hours come from the step count and `steps_per_day`, and
//!    the wind from an optional `weather` schedule or the model's own
//!    setting.
//! 2. **Own a knob it can write better.** `p0` is baked into the model's
//!    per-cell table, so the generic setter would rebuild everything; the
//!    driver claims `model.p0` and uses the exact, cheap
//!    [`WildfireModel::set_p0`] instead, folding the decay in at the same time.
//! 3. **Stop members with a published rule.** Once a simulated day (every
//!    `steps_per_day` steps) a member that is not yet contained is *contained* with
//!    probability `1 / (1 + exp(-(a + b · ln g)))`, where `g` is how much it
//!    grew that day — the containment model FSim uses (Finney et al. 2011),
//!    validated here as E28. `a` and `b` are the genes `contain_a` and
//!    `contain_b`; leave them out and no member is ever contained. The first
//!    day boundary only records the fire's size, so containment draws start on
//!    day two. A contained member keeps its cells but its `p0` becomes 0 for good.
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
use crate::rules::{Neighborhood2D, neighborhood_offsets};
use crate::types::CellType;

/// Free gene: multiplier on the wind speed the forcing supplies.
pub const GENE_WIND_SCALE: &str = "wind_scale";
/// Free gene: degrees added to the forcing's wind *from*-bearing before it
/// is written into the model, per member (E30b). A fixed rotation of the
/// whole schedule is the runner's job (the `wildfire_smc` example's
/// `SMC_WIND_ROT_DEG`, applied to the weather schedule before the driver sees
/// it); this gene is the per-member, *learned* version of the same idea, so each
/// member can trust the reported wind direction by a different amount.
/// Left out of a run's gene list it contributes nothing: `apply` reads it with
/// `unwrap_or(0.0)`.
pub const GENE_WIND_ROT_DEG: &str = "wind_rot_deg";
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
/// `speed_ms` is in m/s and `from_deg` is the compass bearing the wind blows
/// *from* (270 = a west wind).
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
    /// The smallest growth ratio `period_end` will use. At each day boundary
    /// it computes growth `(burned - before) / before` (cells burning or burned
    /// now vs. at the last boundary) and takes its `ln` for the containment
    /// logit; growth below this floor is raised to it (E49 sweeps this value;
    /// the `wildfire_smc` example's `SMC_CONTAIN_GROWTH_FLOOR` sets it).
    /// A member that did not grow at all (growth 0) therefore uses the floor.
    /// A *smaller* floor makes `ln growth` more negative and, because
    /// `contain_b` is always negative (its range is -2.0 to -0.3), makes the
    /// logit larger: stalled members become more certain to be contained.
    /// The default is `1e-4` (`default_contain_growth_floor`).
    ///
    /// **Must be a finite number > 0.** The floor keeps `ln growth` finite for
    /// a member that did not grow. A floor of 0 gives `ln 0 = -inf`, which with a
    /// negative `contain_b` makes the logit `+inf`: the member is contained with
    /// probability 1, silently. The library rejects a bad value in two places:
    /// loading a config fails with a serde error, and a driver built in code
    /// (the field is `pub`) makes `period_end` return `ModelError::InvalidParam`.
    /// `wildfire_smc` also checks `SMC_CONTAIN_GROWTH_FLOOR` itself, for an
    /// earlier and friendlier message.
    ///
    /// `skip_serializing_if`: a driver at the default floor serialises with this
    /// key absent, exactly like a config written before the field existed, so an
    /// old saved config round-trips byte for byte. A non-default floor is still
    /// written, and `#[serde(default = ...)]` reads either shape back in.
    #[serde(
        default = "default_contain_growth_floor",
        deserialize_with = "deserialize_contain_growth_floor",
        skip_serializing_if = "is_default_contain_growth_floor"
    )]
    pub contain_growth_floor: f64,
}

/// True when `floor` is usable: finite and greater than 0.
fn valid_contain_growth_floor(floor: f64) -> bool {
    floor.is_finite() && floor > 0.0
}

/// The message used by both validation points.
fn bad_floor_message(floor: f64) -> String {
    format!("contain_growth_floor must be a finite number > 0, got {floor}")
}

/// Serde hook: read the number, then refuse an unusable one.
fn deserialize_contain_growth_floor<'de, D>(d: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let floor = f64::deserialize(d)?;
    if valid_contain_growth_floor(floor) {
        Ok(floor)
    } else {
        Err(serde::de::Error::custom(bad_floor_message(floor)))
    }
}

fn is_default_contain_growth_floor(floor: &f64) -> bool {
    *floor == default_contain_growth_floor()
}

fn default_steps_per_day() -> u64 {
    50
}

/// The default for [`WildfireDriver::contain_growth_floor`] (`1e-4`). It is a
/// function so the serde default, `Default` and the `skip_serializing_if` check
/// all share one value instead of three copies of the literal. It is `pub` so the
/// `wildfire_smc` example can use it as the default of `SMC_CONTAIN_GROWTH_FLOOR`.
pub fn default_contain_growth_floor() -> f64 {
    1e-4
}

impl Default for WildfireDriver {
    fn default() -> Self {
        WildfireDriver {
            steps_per_day: default_steps_per_day(),
            weather: Vec::new(),
            contain_growth_floor: default_contain_growth_floor(),
        }
    }
}

/// Borrow the wildfire model inside `sim`, or fail with `InvalidParam` if the
/// grid has no model or a different kind of model.
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
    /// The wind `(speed_ms, from_deg)` in force at `hours` from the schedule, if
    /// there is one: the window with the latest `hours` not after now, or the
    /// first window if none has started yet. `None` for an empty schedule.
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
            Gene::float(GENE_WIND_ROT_DEG, -90.0, 90.0, false),
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
        let wind_rot = space.float(genome, GENE_WIND_ROT_DEG).unwrap_or(0.0);
        model.params.wind_speed = speed * wind_scale;
        model.params.wind_from_deg = (from + wind_rot).rem_euclid(360.0);

        // The base p0 comes from the gene when there is one (a child's mutated
        // gene must win over the scratch it inherited from its parent). With
        // no gene it is captured from the model once and remembered in `state`:
        // set_p0 overwrites params.p0, so reading it back later would compound
        // the decay.
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
        if !valid_contain_growth_floor(self.contain_growth_floor) {
            return Err(ModelError::InvalidParam(bad_floor_message(
                self.contain_growth_floor,
            )));
        }
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
            let growth = ((burned as f64 - before) / before).max(self.contain_growth_floor);
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

    /// State correction (E40): rebuild an immigrant's grid from the
    /// observation instead of a parent's history (an *immigrant* is a fresh
    /// member injected into the ensemble; `observed` is the real fire's mask). `sim` arrives with a
    /// fresh genome already written into the model but never stepped, so
    /// every cell this loop does not touch is still exactly the scenario's
    /// original fuel/inert layout — the "fresh scenario grid" the design
    /// calls for.
    ///
    /// `observed`'s cells are read the model-agnostic way the engine
    /// promises: `!= observed.inactive()` means "observed on" here. Three
    /// rules, applied per cell:
    ///
    /// - not observed on: untouched (stays the fresh fuel/inert cell).
    /// - observed on, and it is a fuel cell with at least one *unobserved*
    ///   fuel neighbour (the live rim — still next to something that can
    ///   catch): the burning type, age 0 (`Sim::paint` resets age on a real
    ///   type change).
    /// - observed on, otherwise (the burned interior, or an inert cell
    ///   inside the observed set — a lake the satellite's pixel happened to
    ///   catch, say): the burned type. An inert cell can never satisfy the
    ///   rim rule (it is excluded from the fuel check on both sides), so it
    ///   is never set to the burning type — non-flammable terrain cannot be
    ///   put back on fire just because the mask covers it.
    fn seed_from_observation(&self, sim: &mut Sim, observed: &Sim) -> Result<(), ModelError> {
        let fresh: Vec<CellType> = sim.cells().to_vec();
        let obs_bg = observed.inactive();
        let obs_on: Vec<bool> = observed.cells().iter().map(|&t| t != obs_bg).collect();
        let (w, h) = sim.dims();
        let (burning_t, burned_t, fuels) = {
            let model = model_of(sim)?;
            (
                model.burning_type(),
                model.burned_type(),
                model
                    .params
                    .fuels
                    .iter()
                    .map(|f| CellType::new(&f.name))
                    .collect::<Vec<CellType>>(),
            )
        };
        let is_fuel = |t: CellType| fuels.contains(&t);
        let offsets = neighborhood_offsets(Neighborhood2D::Moore, 1);
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                if !obs_on[idx] {
                    continue;
                }
                let rim = is_fuel(fresh[idx])
                    && offsets.iter().any(|&(dx, dy)| {
                        let (nx, ny) = (x as i64 + i64::from(dx), y as i64 + i64::from(dy));
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            return false;
                        }
                        let nidx = ny as usize * w + nx as usize;
                        !obs_on[nidx] && is_fuel(fresh[nidx])
                    });
                sim.paint(idx, if rim { burning_t } else { burned_t })?;
            }
        }
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
    use crate::external::ParamValue;
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
            spread: "bernoulli".into(),
            arrival_jitter: 0.2,
            wind_law: "exponential".into(),
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
    fn wind_rot_deg_gene_rotates_each_members_wind_independently() {
        // Two members, one ensemble, the same forcing (wind from 90 deg).
        // Pin one member's `wind_rot_deg` to 0 and the other's to 90 by
        // writing the genome directly -- the gene draws a random value per
        // member otherwise, which a test cannot rely on.
        let mut genes = fire_genes();
        genes.push(GeneSpec::range(GENE_WIND_ROT_DEG, -90.0, 90.0));
        let cfg = EnsembleConfig {
            genes,
            ..fire_config(2, WildfireDriver::default())
        };
        let mut e = Ensemble::new(template(8, 8), &cfg).unwrap();
        let space = e.space().clone();
        let idx = space
            .index(GENE_WIND_ROT_DEG)
            .expect("the gene is in this run's list");
        let ws_idx = space
            .index(GENE_WIND_SCALE)
            .expect("wind_scale is in this run's list too");
        // Pin wind_scale equal on both members: it otherwise draws a
        // random value per member, which would confound the wind-speed
        // check below with something this test isn't about.
        e.members_mut()[0].genome.0[ws_idx] = ParamValue::Float(1.0);
        e.members_mut()[1].genome.0[ws_idx] = ParamValue::Float(1.0);
        e.members_mut()[0].genome.0[idx] = ParamValue::Float(0.0);
        e.members_mut()[1].genome.0[idx] = ParamValue::Float(90.0);
        e.set_forcing(forcing(0.0, 4.0, 90.0)).unwrap();
        let members = e.members_mut();
        let p0 = model_params(&mut members[0].sim);
        let p1 = model_params(&mut members[1].sim);
        assert_eq!(
            p0.wind_from_deg, 90.0,
            "rotation 0 leaves the forcing's bearing unchanged"
        );
        assert_eq!(
            p1.wind_from_deg, 180.0,
            "rotation 90 adds to the forcing's bearing, mod 360"
        );
        assert_ne!(
            p0.wind_from_deg, p1.wind_from_deg,
            "two members, same forcing, different wind_rot_deg -> different grid wind"
        );
        assert_eq!(
            p0.wind_speed, p1.wind_speed,
            "the gene rotates direction only, never speed"
        );
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
            ..Default::default()
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
                    ..Default::default()
                },
            ),
        )
        .unwrap();
        plain.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
        plain.step_n(9).unwrap();
        assert_eq!(plain.state_fraction(STATE_CONTAINED), 0.0);
    }

    /// E49: `contain_growth_floor` is the floor
    /// `period_end` applies to a period's growth ratio before taking its
    /// `ln` for the containment logit. Pin `model.p0` to 0 so the seeded
    /// cell can spread to no neighbours -- the tracked (burning +
    /// burned-out) count is exactly 1 at every period boundary, so growth
    /// is exactly 0 *before* the floor and the floor decides the whole
    /// logit. With `contain_a = 0, contain_b = -2` (the most negative slope in
    /// `GENE_CONTAIN_B`'s range of `-2.0` to `-0.3`), a small floor (default `1e-4`, `ln ~ -9.2`) pushes the
    /// logit strongly positive (~18.4, p ~ 1.0: essentially every member
    /// contained); a large floor (`5.0`, `ln ~ 1.6`) pushes it strongly
    /// negative (~-3.2, p ~ 0.04: essentially none). Same genes, same
    /// seed, same two-period run -- only `contain_growth_floor` differs --
    /// so this is the acceptance check that the knob (and not something
    /// else) is what moves the outcome, and that the unset/default value
    /// reproduces the operator's original, always-`1e-4` behaviour.
    #[test]
    fn contain_growth_floor_knob_moves_containment_for_a_stalled_member() {
        let mut genes = fire_genes();
        for g in &mut genes {
            if g.key == "model.p0" {
                *g = GeneSpec::range("model.p0", 0.0, 0.0);
            }
        }
        genes.push(GeneSpec::range(GENE_CONTAIN_A, 0.0, 0.0));
        genes.push(GeneSpec::range(GENE_CONTAIN_B, -2.0, -2.0));

        let run_with_floor = |floor: f64| -> f64 {
            let driver = WildfireDriver {
                steps_per_day: 6,
                weather: vec![],
                contain_growth_floor: floor,
            };
            let cfg = EnsembleConfig {
                members: 30,
                genes: genes.clone(),
                ..fire_config(30, driver)
            };
            let mut e = Ensemble::new(template(16, 16), &cfg).unwrap();
            e.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
            // First period boundary: only records the starting size
            // (before == 0 skips the containment draw).
            e.step_n(6).unwrap();
            assert_eq!(e.state_fraction(STATE_CONTAINED), 0.0);
            // Second period boundary: p0 == 0 means the seeded cell has no
            // neighbours to ignite, so burned/burning count is unchanged --
            // growth is exactly 0 before the floor.
            e.step_n(6).unwrap();
            e.state_fraction(STATE_CONTAINED)
        };

        let default_floor = run_with_floor(default_contain_growth_floor());
        assert!(
            default_floor >= 0.9,
            "default floor 1e-4 should contain almost every stalled member: {default_floor}"
        );
        let wide_floor = run_with_floor(5.0);
        assert!(
            wide_floor <= 0.15,
            "a floor of 5.0 should contain almost no stalled member: {wide_floor}"
        );
    }

    /// Config backward-compat (whole-branch review, after E49):
    /// `contain_growth_floor`'s `skip_serializing_if` must drop the key
    /// entirely for a driver at the default floor (so a driver written
    /// before this field existed still serialises byte-for-byte the same),
    /// and must still write it for a driver at a non-default floor (so the
    /// override round-trips instead of silently reverting to `1e-4`).
    #[test]
    fn contain_growth_floor_is_omitted_only_at_its_default_value() {
        let default_driver = WildfireDriver::default();
        let default_json = serde_json::to_string(&default_driver).unwrap();
        assert!(
            !default_json.contains("contain_growth_floor"),
            "a default driver must not serialise contain_growth_floor: {default_json}"
        );
        let round_tripped: WildfireDriver = serde_json::from_str(&default_json).unwrap();
        assert_eq!(round_tripped, default_driver);

        let overridden = WildfireDriver {
            contain_growth_floor: 1e-3,
            ..WildfireDriver::default()
        };
        let overridden_json = serde_json::to_string(&overridden).unwrap();
        assert!(
            overridden_json.contains(r#""contain_growth_floor":0.001"#),
            "a non-default floor must still serialise: {overridden_json}"
        );
        let round_tripped: WildfireDriver = serde_json::from_str(&overridden_json).unwrap();
        assert_eq!(round_tripped, overridden);
    }

    /// E49, Phase 2: the sweep's own endpoints. The
    /// sweep (`SMC_CONTAIN_GROWTH_FLOOR` in {1e-5, 1e-4, 1e-3}) produced
    /// byte-identical reports, so this pins that the three values it used
    /// really do reach `period_end` and really do change the daily
    /// containment probability of a member whose growth sits below the
    /// floor. Same stalled-member setup as the test above (growth exactly
    /// 0 before the floor). With `contain_a = -4, contain_b = -0.5` the
    /// probabilities are well apart: p = sigmoid(-4 - 0.5 ln f) =
    /// 0.853 at 1e-5, 0.646 at 1e-4, 0.366 at 1e-3. Every run uses the
    /// same seed, so every member sees the same random draw `u` under all
    /// three floors, and "contained" at a larger floor implies "contained"
    /// at a smaller one: the fractions must fall strictly as the floor
    /// rises.
    ///
    /// The second half is the reason a sweep can still come out
    /// identical: with a steep slope (`contain_b = -2`) and `contain_a =
    /// 0`, the same three floors give p = 0.9999999999 / 0.99999999 /
    /// 0.999999 -- all saturated at "certainly contained", so the floor
    /// binds but no draw can land between the probabilities.
    #[test]
    fn sweep_floors_1e5_1e4_1e3_change_containment_unless_the_logit_is_saturated() {
        let contained_at = |a: f64, b: f64, floor: f64| -> f64 {
            let mut genes = fire_genes();
            for g in &mut genes {
                if g.key == "model.p0" {
                    *g = GeneSpec::range("model.p0", 0.0, 0.0);
                }
            }
            genes.push(GeneSpec::range(GENE_CONTAIN_A, a, a));
            genes.push(GeneSpec::range(GENE_CONTAIN_B, b, b));
            let driver = WildfireDriver {
                steps_per_day: 6,
                weather: vec![],
                contain_growth_floor: floor,
            };
            let cfg = EnsembleConfig {
                members: 200,
                genes,
                ..fire_config(200, driver)
            };
            let mut e = Ensemble::new(template(16, 16), &cfg).unwrap();
            e.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
            e.step_n(12).unwrap();
            e.state_fraction(STATE_CONTAINED)
        };

        let lo = contained_at(-4.0, -0.5, 1e-5);
        let mid = contained_at(-4.0, -0.5, 1e-4);
        let hi = contained_at(-4.0, -0.5, 1e-3);
        assert!(
            lo > mid && mid > hi,
            "a stalled member's containment must fall as the floor rises: \
             1e-5 -> {lo}, 1e-4 -> {mid}, 1e-3 -> {hi}"
        );
        // Loose bands around the exact probabilities above (200 members).
        assert!((lo - 0.853).abs() < 0.1, "1e-5: {lo}");
        assert!((mid - 0.646).abs() < 0.1, "1e-4: {mid}");
        assert!((hi - 0.366).abs() < 0.1, "1e-3: {hi}");

        let sat_lo = contained_at(0.0, -2.0, 1e-5);
        let sat_hi = contained_at(0.0, -2.0, 1e-3);
        assert_eq!(sat_lo, 1.0);
        assert_eq!(sat_hi, 1.0, "saturated logit: the floor binds but cannot flip a draw");
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
            ..Default::default()
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
        assert_eq!(d.free_genes().len(), 5);
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

    #[test]
    fn seed_from_observation_marks_the_rim_and_leaves_inert_cells_alone() {
        let (w, h) = (20usize, 20usize);
        let forest = CellType::new("Forest");
        let water = CellType::new("Water"); // not a fuel class: inert.
        let mut cells = vec![forest; w * h];
        // An inert cell sitting right on the burned block's own edge, where
        // a fuel cell in the same spot would qualify as the rim.
        cells[5 * w + 9] = water;

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
            spread: "bernoulli".into(),
            arrival_jitter: 0.2,
            wind_law: "exponential".into(),
        };
        let mut g = Grid2D::new(w, h, 0, cells, Rule2D { subrules: vec![] });
        g.attach_model(Box::new(WildfireModel::new(params, WildfireEnv::default())))
            .unwrap();
        let mut sim = Sim::D2(g);

        // Observed: a 10x10 burned block in the top-left corner; everything
        // else unobserved.
        let mut mask = vec![false; w * h];
        for y in 0..10 {
            for x in 0..10 {
                mask[y * w + x] = true;
            }
        }
        let burning = CellType::new("Burning");
        let inactive = CellType::inactive();
        let obs_cells: Vec<CellType> = mask
            .iter()
            .map(|&b| if b { burning } else { inactive })
            .collect();
        let observed = Sim::D2(Grid2D::new(w, h, 0, obs_cells, Rule2D { subrules: vec![] }));

        WildfireDriver::default()
            .seed_from_observation(&mut sim, &observed)
            .unwrap();

        let burned_t = CellType::new("BurnedOut");
        let idx = |x: usize, y: usize| y * w + x;

        // Burned interior: deep inside the block, every neighbour is also
        // observed on, so there is nothing left unburned to catch from.
        assert_eq!(
            sim.cells()[idx(3, 3)],
            burned_t,
            "interior of the observed block must be BurnedOut"
        );

        // Rim: on the block's own edge, next to an unobserved, unburned
        // Forest cell — the live edge that should still be spreading.
        assert_eq!(sim.cells()[idx(9, 2)], burning, "the rim must be Burning");
        assert_eq!(
            sim.ages()[idx(9, 2)],
            0,
            "a freshly-marked rim cell starts at age 0"
        );

        // Fuel elsewhere: outside the mask, left exactly as the fresh
        // scenario had it — untouched, not even repainted to itself.
        assert_eq!(
            sim.cells()[idx(15, 15)],
            forest,
            "unburned fuel far from the mask is untouched"
        );
        assert_eq!(
            sim.cells()[idx(10, 5)],
            forest,
            "unburned fuel just outside the block is untouched too"
        );

        // An inert cell inside the observed block must never become
        // Burning, even though it sits exactly where a fuel cell would
        // have qualified as the rim (its neighbour at (10, 5) is
        // unobserved, unburned Forest).
        assert_ne!(
            sim.cells()[idx(9, 5)],
            burning,
            "an inert cell must never be marked burning"
        );
        assert_eq!(
            sim.cells()[idx(9, 5)],
            burned_t,
            "an inert-but-observed cell collapses to burned, not left as Water"
        );
    }

    #[test]
    fn missing_tau_gene_never_decays_and_partial_forcing_keeps_the_last_wind_speed() {
        // No `tau_days` gene: `apply`'s decay match has nothing to read for
        // it, so it takes its `_ => 1.0` arm and p0 never decays, however
        // much simulated time passes.
        let genes = vec![
            GeneSpec::log_range("model.p0", 0.08, 0.6),
            GeneSpec::range("model.burn_duration", 5.0, 20.0),
            GeneSpec::range(GENE_WIND_SCALE, 1.0, 1.0),
        ];
        let cfg = EnsembleConfig {
            members: 3,
            genes,
            track: vec!["Burning".into(), "BurnedOut".into()],
            driver: Some(Box::new(WildfireDriver::default())),
            ..EnsembleConfig::default()
        };
        let mut e = Ensemble::new(template(12, 12), &cfg).unwrap();
        e.set_forcing(forcing(0.0, 4.0, 90.0)).unwrap();
        let space = e.space().clone();
        for m in e.members_mut() {
            let genome = m.genome.clone();
            let p0_gene = space.float(&genome, "model.p0").unwrap();
            assert!((model_params(&mut m.sim).p0 - p0_gene).abs() < 1e-12);
        }
        // Ten simulated days later, still no tau_days gene: p0 must still
        // be exactly the gene's own value, not decayed.
        e.set_forcing(forcing(240.0, 4.0, 90.0)).unwrap();
        for m in e.members_mut() {
            let genome = m.genome.clone();
            let p0_gene = space.float(&genome, "model.p0").unwrap();
            assert!(
                (model_params(&mut m.sim).p0 - p0_gene).abs() < 1e-9,
                "no tau_days gene: p0 must never decay"
            );
        }

        // A forcing that reports a wind bearing but no wind speed: the
        // driver falls back to whichever speed the model currently holds
        // (here, the 4.0 m/s the full forcing above wrote, with
        // wind_scale pinned to 1.0) rather than discarding it.
        let mut bearing_only = Forcing::new();
        bearing_only.insert(FORCING_HOURS.into(), 264.0);
        bearing_only.insert(FORCING_WIND_FROM.into(), 180.0);
        e.set_forcing(bearing_only).unwrap();
        for m in e.members_mut() {
            let p = model_params(&mut m.sim);
            assert_eq!(p.wind_speed, 4.0, "falls back to the last known speed");
            assert_eq!(p.wind_from_deg, 180.0);
        }
    }

    /// A bad `contain_growth_floor` (0, negative, NaN, infinite) must be
    /// rejected by the library, both when a config is loaded and, for a driver
    /// built in code, when the first period boundary runs.
    #[test]
    fn invalid_contain_growth_floor_is_rejected() {
        for bad in ["0", "0.0", "-0.001"] {
            let json = format!(r#"{{"contain_growth_floor": {bad}}}"#);
            assert!(
                serde_json::from_str::<WildfireDriver>(&json).is_err(),
                "{bad} should be rejected on load"
            );
        }
        let ok: WildfireDriver = serde_json::from_str(r#"{"contain_growth_floor": 0.001}"#).unwrap();
        assert_eq!(ok.contain_growth_floor, 0.001);

        for bad in [0.0, -1e-3, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let driver = WildfireDriver {
                steps_per_day: 6,
                weather: vec![],
                contain_growth_floor: bad,
            };
            let cfg = fire_config(4, driver);
            let mut e = Ensemble::new(template(8, 8), &cfg).unwrap();
            e.set_forcing(forcing(0.0, 0.0, 270.0)).unwrap();
            assert!(e.step_n(6).is_err(), "{bad} should be rejected at a period end");
        }
    }
}
