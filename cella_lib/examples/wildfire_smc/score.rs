//! The per-observation report rows: one struct per mode's scoring shape.
//! The scoring functions themselves (IoU, Brier) live in
//! `cella_lib::explore::metrics` and are only ever called from the mode
//! that fills these rows in ([`crate::modes::open`], [`crate::nulls`]).

use serde::Serialize;

/// One observation's scores in the ensemble modes (`open`/`assim`/`evolve`):
/// every ensemble diagnostic plus the deterministic nulls, scored beside it
/// at the same window.
#[derive(Serialize)]
pub(crate) struct ObsScore {
    pub(crate) hours: f64,
    pub(crate) obs_burned: u64,
    pub(crate) mean_member_iou: f64,
    pub(crate) best_member_iou: f64,
    pub(crate) consensus_iou: f64,
    pub(crate) union_iou: f64,
    pub(crate) best_threshold_iou: f64,
    pub(crate) best_threshold: f64,
    pub(crate) area_ratio_mean: f64,
    pub(crate) brier_ensemble: f64,
    pub(crate) brier_radial: f64,
    pub(crate) brier_persistence: f64,
    pub(crate) radial_iou: f64,
    pub(crate) persistence_iou: f64,
    /// The Ellipse null (ERA5 wind variant, E41), reported beside the
    /// Circle in every ensemble mode so future tables carry it for free.
    pub(crate) ellipse_iou: f64,
    pub(crate) brier_ellipse: f64,
    /// The two *lagged* nulls (E40 controller finding): "yesterday's
    /// perimeter, as is, is today's forecast" (lagged persistence) and
    /// "grow yesterday's perimeter, matched to today's true area" (lagged
    /// Circle) — the fair dummy competitors for any state-corrected mode,
    /// since both see exactly what state correction sees (the mask at
    /// t_{k-1}), no more. `None` at the very first scored observation,
    /// where there is no previous *observed* window to lag from (only the
    /// ignition mask, already reported as `persistence_iou`/`radial_iou`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lagged_persistence_iou: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) brier_lagged_persistence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lagged_circle_iou: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) brier_lagged_circle: Option<f64>,
    pub(crate) ess: f64,
    pub(crate) p0_mean: f64,
    pub(crate) p0_std: f64,
    pub(crate) tau_mean: f64,
    pub(crate) tau_std: f64,
    pub(crate) dur_mean: f64,
    pub(crate) wind_scale_mean: f64,
    pub(crate) contained_fraction: f64,
    /// E48 (Round 7 Task 3) per-window diagnostics, opt-in via
    /// `SMC_DIAG=1` — omitted from the JSON entirely when unset, so every
    /// field above this one is byte-identical to what it was before this
    /// field existed. See [`crate::diag::WindowDiag`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diag: Option<crate::diag::WindowDiag>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal-but-complete `ObsScore` with every non-`diag` field set to
    /// an arbitrary value and `diag: None` — the acceptance check this
    /// struct's own field carries: SMC_DIAG unset (the default) must not
    /// add a `"diag"` key to the JSON at all, byte-identical to a report
    /// from before this field existed.
    fn sample() -> ObsScore {
        ObsScore {
            hours: 24.0,
            obs_burned: 100,
            mean_member_iou: 0.5,
            best_member_iou: 0.6,
            consensus_iou: 0.5,
            union_iou: 0.7,
            best_threshold_iou: 0.55,
            best_threshold: 0.4,
            area_ratio_mean: 1.0,
            brier_ensemble: 0.1,
            brier_radial: 0.2,
            brier_persistence: 0.3,
            radial_iou: 0.4,
            persistence_iou: 0.1,
            ellipse_iou: 0.45,
            brier_ellipse: 0.15,
            lagged_persistence_iou: None,
            brier_lagged_persistence: None,
            lagged_circle_iou: None,
            brier_lagged_circle: None,
            ess: 10.0,
            p0_mean: 0.2,
            p0_std: 0.05,
            tau_mean: 15.0,
            tau_std: 2.0,
            dur_mean: 12.0,
            wind_scale_mean: 0.8,
            contained_fraction: 0.1,
            diag: None,
        }
    }

    #[test]
    fn diag_field_is_absent_from_the_json_when_none() {
        let json = serde_json::to_string(&sample()).unwrap();
        assert!(
            !json.contains("\"diag\""),
            "diag: None must not serialise a \"diag\" key (SMC_DIAG unset ⇒ byte-identical \
             report to before this field existed); got: {json}"
        );
    }
}

/// One observation's scores in `nulls` mode (E41): the deterministic dummy
/// forecasters only, no ensemble. `ellipse_station_*` are `None` when the
/// scenario has no station file.
#[derive(Serialize)]
pub(crate) struct NullObsScore {
    pub(crate) hours: f64,
    pub(crate) obs_burned: u64,
    pub(crate) persistence_iou: f64,
    pub(crate) brier_persistence: f64,
    pub(crate) radial_iou: f64,
    pub(crate) brier_radial: f64,
    pub(crate) ellipse_era5_iou: f64,
    pub(crate) brier_ellipse_era5: f64,
    pub(crate) ellipse_station_iou: Option<f64>,
    pub(crate) brier_ellipse_station: Option<f64>,
    pub(crate) ellipse_era5x3_iou: f64,
    pub(crate) brier_ellipse_era5x3: f64,
    /// Post-hoc control (not pre-registered — TEST_PLAN v1.8 addendum):
    /// same ERA5 wind and LB as `ellipse_era5`, but centred (no front/back
    /// skew). Separates "which way the fire runs" from "how stretched".
    pub(crate) ellipse_era5_centred_iou: f64,
    pub(crate) brier_ellipse_era5_centred: f64,
}
