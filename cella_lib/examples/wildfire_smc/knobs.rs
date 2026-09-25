//! `SMC_*` environment-variable parsing: the knobs that are read once at
//! the top of `main` and used across every mode (as opposed to a knob like
//! `SMC_FIT_DAYS` or `SMC_MAP_DAYS` that only means something to one mode
//! and so is read where that mode itself lives). See the top-level module
//! doc comment in `main.rs` for what every knob does.

use crate::Scenario;
use cella_lib::{GeneSpec, ParamValue, StateCorrection};

/// Parse an env var as `f64`, or `default` if it is unset or unparsable.
pub(crate) fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Parse an env var as `usize`, or `default` if it is unset or unparsable.
pub(crate) fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Parse an env var as `f64`, or `None` if it is unset or unparsable —
/// for the knobs whose absence means "leave this alone" rather than "use a
/// default value" (see e.g. [`crate::priors::configure_spread`]).
pub(crate) fn env_f64_opt(key: &str) -> Option<f64> {
    std::env::var(key).ok().and_then(|v| v.parse::<f64>().ok())
}

/// `SMC_WIND_SOURCE=era5|station` (Round 7 Task 6/E46): which wind feeds
/// the driver's per-window forcing. `Era5` (default, unset) is the
/// existing behaviour — byte-identical reports to before this knob
/// existed. `Station` replaces each window's ERA5 (speed, from-bearing)
/// with the station log's vector mean over that same window, falling
/// back to that window's own ERA5 entry (counted) where the station log
/// has no samples in range. See `crate::nulls::wind_schedule_for`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum WindSource {
    Era5,
    Station,
}

impl WindSource {
    fn from_env() -> Self {
        Self::from_opt(std::env::var("SMC_WIND_SOURCE").ok().as_deref())
    }

    /// The env-free half of `from_env`, so the parsing rule (unset/"era5"/
    /// anything unrecognised -> `Era5`, only exactly "station" ->
    /// `Station`) is unit-testable without mutating the process's real
    /// environment (tests run in parallel in the same process — see
    /// `wind_source_tests` below).
    fn from_opt(v: Option<&str>) -> Self {
        match v {
            Some("station") => WindSource::Station,
            // Unset, "era5", or anything unrecognised: the pre-existing
            // behaviour, so a typo in the env var is a silent no-op rather
            // than a panic — consistent with every other `SMC_*` knob here.
            _ => WindSource::Era5,
        }
    }

    /// The string the report's `wind_source` field carries.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WindSource::Era5 => "era5",
            WindSource::Station => "station",
        }
    }
}

#[cfg(test)]
mod wind_source_tests {
    use super::WindSource;

    /// TEST_PLAN.md v1.9 E46: "era5 (default) is unchanged behaviour" —
    /// unset must parse the same as before this knob existed.
    #[test]
    fn unset_defaults_to_era5() {
        assert_eq!(WindSource::from_opt(None), WindSource::Era5);
    }

    #[test]
    fn station_parses_to_station() {
        assert_eq!(WindSource::from_opt(Some("station")), WindSource::Station);
    }

    /// A typo or an explicit "era5" is a no-op, not a panic — same rule
    /// every other `SMC_*` knob in this file follows.
    #[test]
    fn unrecognised_value_falls_back_to_era5() {
        assert_eq!(WindSource::from_opt(Some("era5")), WindSource::Era5);
        assert_eq!(WindSource::from_opt(Some("bogus")), WindSource::Era5);
    }
}

/// Every `SMC_*` knob read once at the top of `main`, before any
/// mode-specific dispatch — the mode-specific knobs (`SMC_FIT_DAYS`,
/// `SMC_POP`, `SMC_MAP_DAYS`, `SMC_MAP_REPLAY`, ...) are read where the mode
/// that uses them lives instead, since several of them (`SMC_POP`,
/// `SMC_GENERATIONS`) have different defaults in different modes and
/// folding them in here would risk mixing those defaults up.
pub(crate) struct Knobs {
    pub(crate) wind_rot_deg: f64,
    pub(crate) assim_every: usize,
    /// `open`/`assim` only: stop after this many *scored* observation days
    /// instead of running the whole scenario. `usize::MAX` (unset) runs
    /// every day, same as before this knob existed.
    pub(crate) max_days: usize,
    pub(crate) spot: bool,
    pub(crate) contain: bool,
    pub(crate) tau_off: bool,
    pub(crate) wind_rot_gene: Option<f64>,
    /// Round 7 Task 5 (E45): per-gene mutation-size override on
    /// `wind_rot_deg` only (`SMC_WIND_ROT_SIGMA`), no effect unless
    /// `wind_rot_gene` is also set. `None` (unset) leaves the gene's sigma
    /// at the engine's own default. See `priors::build_genes`.
    pub(crate) wind_rot_sigma: Option<f64>,
    pub(crate) spread: Option<String>,
    pub(crate) wind_law: Option<String>,
    pub(crate) c2: Option<f64>,
    pub(crate) arrival_jitter: Option<f64>,
    pub(crate) steps_scale: f64,
    pub(crate) seed: u64,
    pub(crate) beta: f64,
    pub(crate) sigma: f64,
    pub(crate) immigrants: f64,
    pub(crate) crossover: f64,
    pub(crate) immigrant_reset: bool,
    pub(crate) immigrant_reset_gate: Option<f64>,
    pub(crate) state_correction: StateCorrection,
    /// E48 (Round 7 Task 3): opt-in per-window diagnostics (consensus vs
    /// truth head/flank decomposition, learned-gene medians, ERA5/station
    /// wind vectors) added to `ObsScore.diag` — `None`/omitted from the
    /// JSON when this is unset, so every existing field is unaffected. See
    /// `crate::diag`.
    pub(crate) diag: bool,
    /// Round 7 Task 6 (E46): `SMC_WIND_SOURCE`. See [`WindSource`].
    pub(crate) wind_source: WindSource,
    /// Round 7 Task 7 (E49): `SMC_CONTAIN_GROWTH_FLOOR`, an opt-in override
    /// of the containment operator's growth floor
    /// (`WildfireDriver::contain_growth_floor` — see that field's doc
    /// comment for what it does). Unset parses to the operator's
    /// pre-existing hard-coded value (`1e-4`,
    /// `cella_lib::wildfire::driver::default_contain_growth_floor`'s
    /// twin default here), so every report from before this knob existed
    /// is reproduced byte-identically.
    pub(crate) contain_growth_floor: f64,
}

impl Knobs {
    pub(crate) fn from_env() -> Self {
        Knobs {
            wind_rot_deg: env_f64("SMC_WIND_ROT_DEG", 0.0),
            assim_every: env_f64("SMC_ASSIM_EVERY", 1.0).max(1.0) as usize,
            max_days: env_usize("SMC_MAX_DAYS", usize::MAX),
            spot: env_f64("SMC_SPOT", 0.0) > 0.0,
            contain: env_f64("SMC_CONTAIN", 0.0) > 0.0,
            tau_off: env_f64("SMC_TAU_OFF", 0.0) > 0.0,
            wind_rot_gene: env_f64_opt("SMC_WIND_ROT_GENE"),
            wind_rot_sigma: env_f64_opt("SMC_WIND_ROT_SIGMA"),
            spread: std::env::var("SMC_SPREAD").ok(),
            wind_law: std::env::var("SMC_WIND_LAW").ok(),
            c2: env_f64_opt("SMC_C2"),
            arrival_jitter: env_f64_opt("SMC_ARRIVAL_JITTER"),
            steps_scale: env_f64("SMC_STEPS_SCALE", 1.0),
            seed: env_f64("SMC_SEED", 0.0) as u64,
            beta: env_f64("SMC_BETA", 10.0),
            sigma: env_f64("SMC_SIGMA", 0.2),
            immigrants: env_f64("SMC_IMMIGRANTS", 0.0),
            crossover: env_f64("SMC_CROSSOVER", 0.0),
            immigrant_reset: env_f64("SMC_IMM_RESET", 0.0) > 0.0,
            immigrant_reset_gate: std::env::var("SMC_IMM_RESET_GATE")
                .ok()
                .and_then(|v| v.parse::<f64>().ok()),
            state_correction: match std::env::var("SMC_STATE_CORRECTION").as_deref() {
                Ok("all") => StateCorrection::All,
                Ok("immigrants") => StateCorrection::Immigrants,
                Ok("none") => StateCorrection::None,
                // SMC_STATE_CORRECTION unset or unrecognised: fall back to the
                // older SMC_IMM_SOURCE knob (E40's original runner sets it),
                // then to no correction at all.
                _ => match std::env::var("SMC_IMM_SOURCE").as_deref() {
                    Ok("observed") => StateCorrection::Immigrants,
                    _ => StateCorrection::None,
                },
            },
            diag: env_f64("SMC_DIAG", 0.0) > 0.0,
            wind_source: WindSource::from_env(),
            contain_growth_floor: env_f64(
                "SMC_CONTAIN_GROWTH_FLOOR",
                cella_lib::wildfire::driver::default_contain_growth_floor(),
            ),
        }
    }
}

/// A gene pinned to one value, so an `open` ensemble runs one genome with
/// many seeds.
pub(crate) fn pinned(key: &str, value: &ParamValue) -> GeneSpec {
    let v = match value {
        ParamValue::Float(x) => *x,
        ParamValue::Int(i) => *i as f64,
        other => panic!("cannot pin a {other:?} gene"),
    };
    GeneSpec::range(key, v, v)
}

/// `SMC_STEPS_SCALE` (E30b): multiply the scenario's own `steps_per_hour`
/// in place, before anything else in `main` reads it. Every later use of
/// `sc.steps_per_hour` — the run loop's window step-counts, `observation_steps`,
/// and (via [`steps_per_day_from`], computed from this same field right
/// after) the wildfire driver's `steps_per_day` — is one field read, so
/// scaling it here once keeps the forcing schedule and the driver's
/// containment period in sync automatically: a 4x clock still spans one
/// day of weather in one observation window, just in four times the
/// steps. `scale <= 0` is a caller error (nothing here defends against a
/// zero or negative clock); the default of 1.0 is a no-op.
pub(crate) fn apply_steps_scale(sc: &mut Scenario, scale: f64) {
    sc.steps_per_hour *= scale;
}

/// Steps per simulated day from the scenario's (possibly scaled)
/// `steps_per_hour`, rounded to the nearest whole step and floored at 1 so
/// a driver never gets a zero-length period.
pub(crate) fn steps_per_day_from(sc: &Scenario) -> u64 {
    (24.0 * sc.steps_per_hour).round().max(1.0) as u64
}

/// E30b: `SMC_STEPS_SCALE` must scale the scenario's clock (and, through
/// it, the wildfire driver's `steps_per_day`) together. See
/// `apply_steps_scale` and `steps_per_day_from`. The matching
/// `SMC_WIND_ROT_GENE` tests live in `priors.rs` beside `build_genes`,
/// which is the function that knob actually changes.
#[cfg(test)]
mod e30b_steps_scale_tests {
    use super::*;
    use crate::modes::evolve::observation_steps;
    use crate::{GridMeta, Truth};
    use cella_lib::MemberDriver;
    use cella_lib::wildfire::driver::WildfireDriver;

    fn tiny_scenario(steps_per_hour: f64) -> Scenario {
        Scenario {
            format_version: 2,
            id: "test".into(),
            grid: GridMeta {
                width: 4,
                height: 4,
            },
            wind: vec![],
            steps_per_hour,
        }
    }

    /// E30's own clock: 50 ticks/day, i.e. steps_per_hour = 50 / 24.
    const E30_STEPS_PER_HOUR: f64 = 50.0 / 24.0;

    #[test]
    fn steps_scale_of_one_is_a_no_op() {
        let mut sc = tiny_scenario(E30_STEPS_PER_HOUR);
        apply_steps_scale(&mut sc, 1.0);
        assert_eq!(steps_per_day_from(&sc), 50);
    }

    /// The brief's own acceptance check: SMC_STEPS_SCALE=4 turns E30's 50
    /// ticks/day into 200, and a 24 h observation window -- computed the
    /// same way `observation_steps` computes a window's step count, from
    /// `sc.steps_per_hour` directly -- is 200 steps at that same scale, so
    /// one window still spans exactly one day of forcing.
    #[test]
    fn smc_steps_scale_of_4_quadruples_the_clock_and_keeps_windows_in_sync() {
        let mut sc = tiny_scenario(E30_STEPS_PER_HOUR);
        apply_steps_scale(&mut sc, 4.0);
        let steps_per_day = steps_per_day_from(&sc);
        assert_eq!(steps_per_day, 200, "4x E30's clock: 50 -> 200 ticks/day");

        let truth = Truth {
            format_version: 2,
            observed_at: vec![0.0, 24.0],
            arrival_hours: vec![],
        };
        let windows = observation_steps(&sc, &truth);
        assert_eq!(
            windows,
            vec![(200, 24.0)],
            "a 24h window is 200 steps at SMC_STEPS_SCALE=4"
        );

        let driver = WildfireDriver {
            steps_per_day,
            weather: vec![],
            ..Default::default()
        };
        assert_eq!(
            driver.period_steps(),
            Some(200),
            "the driver's containment period matches the scaled clock"
        );
    }
}
