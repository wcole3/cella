//! Per-window diagnostics (E48, Round 7 Task 3): opt-in via `SMC_DIAG=1`,
//! wired into [`crate::modes::open::run`]'s scoring loop as `ObsScore.diag`
//! (`score::ObsScore`) — added fields only, no existing field changes.
//!
//! Three things live here: the ensemble's per-window learned-gene medians
//! (`median_gene`, since [`cella_lib::Ensemble::genome_stats`] gives mean/
//! sd/min/max but not a median), the ignition centroid and a head-vs-flank
//! classifier for the consensus-vs-truth miss and false-positive cells
//! (`centroid`, `head_flank_decompose`), and the per-window diagnostic row
//! itself (`WindowDiag`). The ERA5/station wind vectors for a window are
//! read by the caller (`modes::open::run` already has the ERA5 entry in
//! hand; the station log is loaded once via [`crate::nulls::load_station`]
//! and read per window via [`crate::nulls::station_vector_mean`]) and
//! passed straight into `WindowDiag`, not recomputed here.

use std::collections::BTreeMap;

use cella_lib::ParamValue;
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
pub(crate) fn median_gene(genomes: &[BTreeMap<String, ParamValue>], key: &str) -> Option<f64> {
    let mut v: Vec<f64> = genomes
        .iter()
        .filter_map(|g| match g.get(key) {
            Some(ParamValue::Float(x)) => Some(*x),
            Some(ParamValue::Int(i)) => Some(*i as f64),
            _ => None,
        })
        .collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).expect("gene values are finite"));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
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
/// its displacement from `centroid` has a positive dot product with the
/// unit vector at `toward_rad` (the window's ERA5 "toward" bearing,
/// radians, grid convention: 0 = +x turning toward +y — see
/// [`cella_lib::wildfire::wind_toward_grid_deg`]): "downwind" if positive,
/// "cross-wind" otherwise (this bucket also holds ties at exactly zero and
/// anything upwind — the brief only asks for two buckets, not three).
/// `consensus`/`truth` and `w` (grid width) must agree with `centroid`'s.
pub(crate) fn head_flank_decompose(
    consensus: &[bool],
    truth: &[bool],
    w: usize,
    centroid: (f64, f64),
    toward_rad: f64,
) -> HeadFlankCounts {
    let (cx, cy) = centroid;
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
}
