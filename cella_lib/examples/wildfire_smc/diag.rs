//! Per-window diagnostics (E48, Round 7 Task 3): opt-in via `SMC_DIAG=1`,
//! wired into [`crate::modes::open::run`]'s scoring loop as `ObsScore.diag`
//! (`score::ObsScore`) — added fields only, no existing field changes.
//!
//! Three things live here: the ensemble's per-window learned-gene medians
//! (`median_gene`, since [`cella_lib::Ensemble::genome_stats`] gives mean/
//! sd/min/max but not a median) and spread (`iqr_gene`, Round 7 Task 5:
//! the interquartile range -- Q3 minus Q1 -- of a gene over the current
//! members, a robust width that (unlike sd) is not pulled around by one
//! outlier member), the ignition centroid and a head-vs-flank classifier
//! for the consensus-vs-truth miss and false-positive cells (`centroid`,
//! `head_flank_decompose`), and the per-window diagnostic row itself
//! (`WindowDiag`). The ERA5/station wind vectors for a window are read by
//! the caller (`modes::open::run` already has the ERA5 entry in hand; the
//! station log is loaded once via [`crate::nulls::load_station`] and read
//! per window via [`crate::nulls::station_vector_mean`]) and passed
//! straight into `WindowDiag`, not recomputed here.

use std::collections::BTreeMap;

use cella_lib::wildfire::driver::{
    GENE_CONTAIN_A, GENE_CONTAIN_B, STATE_BURNED_AT_DAY_START, STATE_CONTAINED,
};
use cella_lib::{CellType, Ensemble, ParamValue};
use serde::Serialize;

/// One window's E48 diagnostics: the ERA5 wind vector, the station vector
/// mean (`None` when the fire has no station file), the ensemble's learned
/// gene medians at this point in the run, and the head-vs-flank
/// decomposition of the consensus-vs-truth miss and false-positive cells.
#[derive(Serialize)]
pub(crate) struct WindowDiag {
    pub(crate) era5_speed_ms: f64,
    /// Compass "from" bearing (degrees), including any fixed `SMC_WIND_ROT_DEG`.
    pub(crate) era5_from_deg: f64,
    /// Grid "toward" bearing (degrees) — the direction the wind blows,
    /// converted the same way [`cella_lib::wildfire::wind_toward_grid_deg`]
    /// does, and the vector the head/flank classifier below is built from.
    pub(crate) era5_toward_deg: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) station_speed_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) station_toward_deg: Option<f64>,
    /// Median `model.p0` over the ensemble's members at this window.
    pub(crate) p0_median: f64,
    /// Median `wind_scale` over the ensemble's members at this window.
    pub(crate) wind_scale_median: f64,
    /// Median `wind_rot_deg` over the ensemble's members at this window;
    /// `None` unless `SMC_WIND_ROT_GENE` is set (Arm A has no such gene).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) wind_rot_deg_median: Option<f64>,
    /// Round 7 Task 5 (E45): interquartile range (Q3 − Q1) of `wind_rot_deg`
    /// over the ensemble's members at this window -- the posterior
    /// *spread*, as opposed to `wind_rot_deg_median`'s central tendency.
    /// `None` under the same condition as `wind_rot_deg_median` (the gene
    /// absent from this run's gene list), same "absent means not part of
    /// this run" convention as the median field beside it. A learned
    /// bearing should narrow this over the windows of a run; per-member
    /// angular diversity that survives selection should not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) wind_rot_deg_iqr: Option<f64>,
    /// Truth-1/consensus-0 cells whose displacement from the ignition
    /// centroid has a positive dot product with the window's ERA5 "toward"
    /// vector.
    pub(crate) downwind_miss: u64,
    /// Truth-1/consensus-0 cells that are not downwind (cross-wind or
    /// upwind of the ignition centroid).
    pub(crate) crosswind_miss: u64,
    /// Truth-0/consensus-1 cells, downwind of the ignition centroid.
    pub(crate) downwind_false_positive: u64,
    /// Truth-0/consensus-1 cells, not downwind of the ignition centroid.
    pub(crate) crosswind_false_positive: u64,
    /// Round 7 Task 7 (E49): the smallest raw growth ratio (before the
    /// growth floor is applied) among the containment draws in
    /// `contain_draws`; `None` when no still-burning member was drawn for
    /// since the previous scored window (the key is then omitted, same
    /// convention as `wind_rot_deg_median`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min_growth_uncontained: Option<f64>,
    /// Round 7 Task 7 (E49): every containment draw the driver made since
    /// the previous scored window, one row per draw -- enough to compute,
    /// offline, the containment probability each draw would have had
    /// under any other growth floor.
    pub(crate) contain_draws: Vec<ContainDraw>,
}

/// One daily containment draw (E49), recorded from outside the driver so
/// the driver itself is unchanged: the member's burned count at the start
/// and end of the period, the raw growth ratio `(burned - before) /
/// before` *before* the floor, the member's `contain_a`/`contain_b` genes,
/// and whether the draw contained it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct ContainDraw {
    pub(crate) before: f64,
    pub(crate) burned: f64,
    pub(crate) growth_raw: f64,
    pub(crate) contain_a: f64,
    pub(crate) contain_b: f64,
    pub(crate) contained: bool,
}

/// Step `ens` exactly `n` times -- the same `Ensemble::step` calls
/// `step_n` makes, so the run is unchanged -- and, at every step that ends
/// a containment period, append one [`ContainDraw`] per member the driver
/// draws for. The driver draws only for a member that has both
/// containment genes, a positive burned count at the previous boundary
/// (`STATE_BURNED_AT_DAY_START`) and no `contained` flag yet; this reads
/// exactly those same inputs from the member's state just before the
/// boundary step and the outcome just after it. The burned count is taken
/// with `burnt` (Burning + BurnedOut), the same two types the driver counts.
pub(crate) fn step_recording_contain_draws(
    ens: &mut Ensemble,
    n: u64,
    period: u64,
    burnt: &[CellType; 2],
    out: &mut Vec<ContainDraw>,
) {
    for _ in 0..n {
        let ends_period = period > 0 && (ens.step_count() + 1).is_multiple_of(period);
        // (member index, before, a, b) for every member the driver will draw for.
        let pending: Vec<(usize, f64, f64, f64)> = if ends_period {
            let space = ens.space();
            ens.members()
                .iter()
                .enumerate()
                .filter_map(|(i, m)| {
                    let a = space.float(&m.genome, GENE_CONTAIN_A)?;
                    let b = space.float(&m.genome, GENE_CONTAIN_B)?;
                    let before = m.state.get(STATE_BURNED_AT_DAY_START).unwrap_or(0.0);
                    (before > 0.0 && !m.state.flag(STATE_CONTAINED)).then_some((i, before, a, b))
                })
                .collect()
        } else {
            Vec::new()
        };
        ens.step().expect("members step");
        for (i, before, a, b) in pending {
            let burned = ens.member_mask(i, burnt).iter().filter(|x| **x).count() as f64;
            out.push(ContainDraw {
                before,
                burned,
                growth_raw: (burned - before) / before,
                contain_a: a,
                contain_b: b,
                contained: ens.members()[i].state.flag(STATE_CONTAINED),
            });
        }
    }
}

/// The median of a numeric gene over `genomes` (one `BTreeMap` per member,
/// [`cella_lib::Ensemble::genomes`]'s own shape — taken as a slice rather
/// than the `Ensemble` itself so this is plain, table-driven and testable
/// without standing up a live ensemble), or `None` if the gene isn't part
/// of this run's gene list (e.g. `wind_rot_deg` when `SMC_WIND_ROT_GENE` is
/// unset) — same "absent means not part of this run" convention as
/// [`cella_lib::Ensemble::genome_stats`], which this complements: that
/// gives mean/sd/min/max, this gives the median the mean can hide a skew
/// behind.
/// The numeric values of `key` across `genomes`, sorted. Shared by
/// `median_gene` and `iqr_gene` so both read the same "absent gene" and
/// NaN-sorting rules from one place.
fn sorted_gene_values(genomes: &[BTreeMap<String, ParamValue>], key: &str) -> Vec<f64> {
    let mut v: Vec<f64> = genomes
        .iter()
        .filter_map(|g| match g.get(key) {
            Some(ParamValue::Float(x)) => Some(*x),
            Some(ParamValue::Int(i)) => Some(*i as f64),
            _ => None,
        })
        .collect();
    // total_cmp (not partial_cmp) so a stray NaN gene value sorts to one
    // end instead of aborting the whole run -- diag is a read-only
    // diagnostic, it should never be why a batch panics.
    v.sort_by(f64::total_cmp);
    v
}

pub(crate) fn median_gene(genomes: &[BTreeMap<String, ParamValue>], key: &str) -> Option<f64> {
    let v = sorted_gene_values(genomes, key);
    if v.is_empty() {
        return None;
    }
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// Linear-interpolation quantile of a *sorted* slice (the same method
/// numpy's default `interpolation="linear"` and Excel's `PERCENTILE.INC`
/// use): index `q * (n - 1)`, interpolating between the two neighbouring
/// values when that index is not a whole number. `sorted` must be sorted
/// ascending and non-empty.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let pos = q * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = pos - lo as f64;
        sorted[lo] + frac * (sorted[hi] - sorted[lo])
    }
}

/// The interquartile range (Q3 − Q1, linear-interpolation quantiles) of a
/// numeric gene over `genomes` -- the ensemble's posterior *spread* at
/// this window, to sit beside [`median_gene`]'s central tendency. `None`
/// under the same "gene absent from this run's gene list" condition as
/// `median_gene` (e.g. `wind_rot_deg` when `SMC_WIND_ROT_GENE` is unset).
/// A single member (`n == 1`) has no spread to speak of; `quantile`
/// returns that one value for both Q1 and Q3 in that case, so the IQR is
/// `0.0`, not `None` -- there *is* a gene, it just isn't spread out.
pub(crate) fn iqr_gene(genomes: &[BTreeMap<String, ParamValue>], key: &str) -> Option<f64> {
    let v = sorted_gene_values(genomes, key);
    if v.is_empty() {
        return None;
    }
    Some(quantile(&v, 0.75) - quantile(&v, 0.25))
}

/// The centroid (mean column, mean row) of a boolean mask's `true` cells,
/// `(0.0, 0.0)` if the mask is empty (never hit in practice: the ignition
/// mask this is called on always has at least one burning cell).
pub(crate) fn centroid(mask: &[bool], w: usize) -> (f64, f64) {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for (i, &b) in mask.iter().enumerate() {
        if b {
            sx += (i % w) as f64;
            sy += (i / w) as f64;
            n += 1.0;
        }
    }
    if n == 0.0 { (0.0, 0.0) } else { (sx / n, sy / n) }
}

/// Counts from [`head_flank_decompose`]: the four cells of the 2×2 (miss /
/// false-positive) × (downwind / cross-wind) table.
pub(crate) struct HeadFlankCounts {
    pub(crate) downwind_miss: u64,
    pub(crate) crosswind_miss: u64,
    pub(crate) downwind_false_positive: u64,
    pub(crate) crosswind_false_positive: u64,
}

/// Classifies every cell where `consensus` and `truth` disagree by whether
/// its displacement from `origin` has a positive dot product with the
/// unit vector at `toward_rad` (the window's ERA5 "toward" bearing,
/// radians, grid convention: 0 = +x turning toward +y — see
/// [`cella_lib::wildfire::wind_toward_grid_deg`]): "downwind" if positive,
/// "cross-wind" otherwise (this bucket also holds ties at exactly zero and
/// anything upwind — the brief only asks for two buckets, not three).
/// `consensus`/`truth` and `w` (grid width) must agree with `origin`'s.
/// `origin` is the ignition centroid in every caller ([`centroid`] above,
/// computed once for the whole run) but this function itself doesn't care
/// where the point comes from, hence the more generic parameter name (it
/// used to be called `centroid`, shadowing the function of that name).
pub(crate) fn head_flank_decompose(
    consensus: &[bool],
    truth: &[bool],
    w: usize,
    origin: (f64, f64),
    toward_rad: f64,
) -> HeadFlankCounts {
    let (cx, cy) = origin;
    let (tx, ty) = (toward_rad.cos(), toward_rad.sin());
    let mut counts = HeadFlankCounts {
        downwind_miss: 0,
        crosswind_miss: 0,
        downwind_false_positive: 0,
        crosswind_false_positive: 0,
    };
    for (i, (&c, &t)) in consensus.iter().zip(truth.iter()).enumerate() {
        if c == t {
            continue;
        }
        let x = (i % w) as f64;
        let y = (i / w) as f64;
        let dot = (x - cx) * tx + (y - cy) * ty;
        let downwind = dot > 0.0;
        match (t, c) {
            (true, false) if downwind => counts.downwind_miss += 1,
            (true, false) => counts.crosswind_miss += 1,
            (false, true) if downwind => counts.downwind_false_positive += 1,
            (false, true) => counts.crosswind_false_positive += 1,
            _ => unreachable!("t == c already skipped above"),
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centroid_of_a_single_cell_is_that_cell() {
        let w = 4;
        let mut mask = vec![false; 16];
        mask[2 + 1 * w] = true; // (x=2, y=1)
        assert_eq!(centroid(&mask, w), (2.0, 1.0));
    }

    #[test]
    fn centroid_of_an_empty_mask_is_the_origin() {
        let mask = vec![false; 9];
        assert_eq!(centroid(&mask, 3), (0.0, 0.0));
    }

    #[test]
    fn centroid_averages_two_symmetric_cells() {
        let w = 5;
        let mut mask = vec![false; 25];
        mask[0] = true; // (0, 0)
        mask[4 + 4 * w] = true; // (4, 4)
        assert_eq!(centroid(&mask, w), (2.0, 2.0));
    }

    fn row(pairs: &[(&str, f64)]) -> BTreeMap<String, ParamValue> {
        pairs
            .iter()
            .map(|&(k, v)| (k.to_string(), ParamValue::Float(v)))
            .collect()
    }

    #[test]
    fn median_gene_odd_count_is_the_middle_value() {
        let genomes = vec![
            row(&[("model.p0", 0.3)]),
            row(&[("model.p0", 0.1)]),
            row(&[("model.p0", 0.2)]),
        ];
        assert_eq!(median_gene(&genomes, "model.p0"), Some(0.2));
    }

    #[test]
    fn median_gene_even_count_averages_the_two_middle_values() {
        let genomes = vec![
            row(&[("wind_scale", 0.1)]),
            row(&[("wind_scale", 0.4)]),
            row(&[("wind_scale", 0.2)]),
            row(&[("wind_scale", 0.3)]),
        ];
        let m = median_gene(&genomes, "wind_scale").unwrap();
        assert!((m - 0.25).abs() < 1e-12, "got {m}");
    }

    /// Round 7 Task 5's own acceptance check: a known 10-value vector,
    /// linear-interpolation quartiles computed by hand. Sorted 1..=10:
    /// Q1 index = 0.25 * 9 = 2.25 -> between v[2]=3 and v[3]=4, frac 0.25
    /// -> 3.25. Q3 index = 0.75 * 9 = 6.75 -> between v[6]=7 and v[7]=8,
    /// frac 0.75 -> 7.75. IQR = 7.75 - 3.25 = 4.5.
    #[test]
    fn iqr_gene_matches_a_known_vector_computed_by_hand() {
        let genomes: Vec<_> = (1..=10)
            .map(|i| row(&[("wind_rot_deg", i as f64)]))
            .collect();
        let iqr = iqr_gene(&genomes, "wind_rot_deg").unwrap();
        assert!((iqr - 4.5).abs() < 1e-12, "got {iqr}");
    }

    #[test]
    fn iqr_gene_is_zero_for_a_single_member_not_none() {
        let genomes = vec![row(&[("wind_rot_deg", 12.0)])];
        assert_eq!(iqr_gene(&genomes, "wind_rot_deg"), Some(0.0));
    }

    #[test]
    fn iqr_gene_is_none_when_the_key_is_absent_from_every_member() {
        // Same convention as median_gene: SMC_DIAG on but SMC_WIND_ROT_GENE
        // unset (Arm A) must report "no such gene," not a false IQR of 0.
        let genomes = vec![row(&[("model.p0", 0.2)]), row(&[("model.p0", 0.3)])];
        assert_eq!(iqr_gene(&genomes, "wind_rot_deg"), None);
    }

    /// `WindowDiag.wind_rot_deg_iqr: None` (Arm A, or any run without
    /// `SMC_WIND_ROT_GENE`, even with `SMC_DIAG=1` on) must not serialise a
    /// `"wind_rot_deg_iqr"` key at all -- same `skip_serializing_if`
    /// convention as `wind_rot_deg_median` beside it, and the field-level
    /// half of "IQR absent when the gene isn't part of the run."
    #[test]
    fn wind_rot_deg_iqr_is_absent_from_the_json_when_none() {
        let diag = WindowDiag {
            era5_speed_ms: 1.0,
            era5_from_deg: 0.0,
            era5_toward_deg: 180.0,
            station_speed_ms: None,
            station_toward_deg: None,
            p0_median: 0.2,
            wind_scale_median: 1.0,
            wind_rot_deg_median: None,
            wind_rot_deg_iqr: None,
            downwind_miss: 0,
            crosswind_miss: 0,
            downwind_false_positive: 0,
            crosswind_false_positive: 0,
            min_growth_uncontained: None,
            contain_draws: Vec::new(),
        };
        let json = serde_json::to_string(&diag).unwrap();
        assert!(
            !json.contains("wind_rot_deg_iqr"),
            "None must omit the key entirely, not serialise null: {json}"
        );
        assert!(!json.contains("wind_rot_deg_median"), "median has the same convention: {json}");
        assert!(
            !json.contains("min_growth_uncontained"),
            "E49's min growth has the same convention: {json}"
        );
    }

    #[test]
    fn median_gene_is_none_when_the_key_is_absent_from_every_member() {
        // wind_rot_deg is absent from every member's genome when
        // SMC_WIND_ROT_GENE is unset (Arm A) -- median_gene must say so
        // rather than defaulting to 0.0, since 0.0 would misreport "the
        // gene learned no rotation" instead of "there is no gene".
        let genomes = vec![row(&[("model.p0", 0.2)]), row(&[("model.p0", 0.3)])];
        assert_eq!(median_gene(&genomes, "wind_rot_deg"), None);
    }

    #[test]
    fn head_flank_classifies_downwind_vs_crosswind_misses_and_false_positives() {
        // A 5x1 strip, centroid at x=2, wind blowing toward +x (toward_rad
        // = 0). Truth = {0, 1, 2}, consensus = {2, 3}: cell 0/1 are missed
        // (truth 1, consensus 0), cell 3 is a false positive (truth 0,
        // consensus 1); cell 2 agrees and is skipped; cell 4 agrees (both
        // false) and is skipped.
        let w = 5;
        let truth = vec![true, true, true, false, false];
        let consensus = vec![false, false, true, true, false];
        let counts = head_flank_decompose(&consensus, &truth, w, (2.0, 0.0), 0.0);
        // Cell 0: dx = -2 (upwind/crosswind bucket). Cell 1: dx = -1 (same).
        assert_eq!(counts.downwind_miss, 0);
        assert_eq!(counts.crosswind_miss, 2);
        // Cell 3: dx = +1, downwind.
        assert_eq!(counts.downwind_false_positive, 1);
        assert_eq!(counts.crosswind_false_positive, 0);
    }

    #[test]
    fn head_flank_agreeing_cells_are_never_counted() {
        let w = 3;
        let truth = vec![true, true, true, true, true, true, true, true, true];
        let consensus = truth.clone();
        let counts = head_flank_decompose(&consensus, &truth, w, (1.0, 1.0), 0.0);
        assert_eq!(counts.downwind_miss, 0);
        assert_eq!(counts.crosswind_miss, 0);
        assert_eq!(counts.downwind_false_positive, 0);
        assert_eq!(counts.crosswind_false_positive, 0);
    }

    /// E49: recording the containment draws must not change the run. Two
    /// identical ensembles on a small burning grid, one stepped with
    /// `Ensemble::step_n`, the other with `step_recording_contain_draws`:
    /// every member's burned mask and the contained fraction must match,
    /// the recorder must have seen draws, and each recorded outcome must
    /// be consistent with its own growth (a contained member can only
    /// have been drawn for once).
    #[test]
    fn recording_contain_draws_leaves_the_run_unchanged() {
        use cella_lib::config::{CellaConfig, Config2D};
        use cella_lib::explore::driver::Forcing;
        use cella_lib::wildfire::driver::{FORCING_HOURS, WildfireDriver};
        use cella_lib::wildfire::{FuelClass, WildfireEnv, WildfireModel, WildfireParams};
        use cella_lib::{EnsembleConfig, GeneSpec, Rule2D};

        let (w, h) = (16usize, 16usize);
        let build = || {
            let params = WildfireParams {
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
                burn_duration: 3,
                spotting: None,
                burning_name: None,
                burned_name: None,
                spread: "bernoulli".into(),
                arrival_jitter: 0.2,
                wind_law: "exponential".into(),
            };
            let mut initial = vec!["Forest".to_string(); w * h];
            initial[(h / 2) * w + w / 2] = "Burning".into();
            let cfg = CellaConfig::D2(Config2D {
                width: w,
                height: h,
                history_limit: 1,
                initial,
                rule: Rule2D { subrules: vec![] },
                model: Some(Box::new(WildfireModel::new(params, WildfireEnv::default()))),
                ..Config2D::default()
            });
            let ens_cfg = EnsembleConfig {
                members: 12,
                genes: vec![
                    GeneSpec::log_range("model.p0", 0.05, 0.6),
                    GeneSpec::range(GENE_CONTAIN_A, -4.0, -1.0),
                    GeneSpec::range(GENE_CONTAIN_B, -2.0, -0.3),
                ],
                track: vec!["Burning".into(), "BurnedOut".into()],
                driver: Some(Box::new(WildfireDriver {
                    steps_per_day: 4,
                    ..WildfireDriver::default()
                })),
                ..EnsembleConfig::default()
            };
            let mut e = Ensemble::new(cfg.build_sim().unwrap(), &ens_cfg).unwrap();
            let mut f = Forcing::new();
            f.insert(FORCING_HOURS.into(), 0.0);
            e.set_forcing(f).unwrap();
            e
        };
        let burnt = [CellType::new("Burning"), CellType::new("BurnedOut")];

        let mut plain = build();
        plain.step_n(24).unwrap();
        let mut recorded = build();
        let mut draws = Vec::new();
        step_recording_contain_draws(&mut recorded, 24, 4, &burnt, &mut draws);

        for i in 0..plain.len() {
            assert_eq!(plain.member_mask(i, &burnt), recorded.member_mask(i, &burnt));
        }
        assert_eq!(
            plain.state_fraction(STATE_CONTAINED),
            recorded.state_fraction(STATE_CONTAINED)
        );
        assert!(!draws.is_empty(), "a burning ensemble must draw for containment");
        let contained_draws = draws.iter().filter(|d| d.contained).count() as f64;
        assert_eq!(
            contained_draws / plain.len() as f64,
            recorded.state_fraction(STATE_CONTAINED),
            "each contained member is drawn for, and contained, exactly once"
        );
        for d in &draws {
            assert!(d.before > 0.0 && d.burned >= d.before, "{d:?}");
            assert_eq!(d.growth_raw, (d.burned - d.before) / d.before);
        }
    }
}
