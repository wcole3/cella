# Wildfire Model Validation — Test Plan (v1)

**Guiding principle: the goal is the best answer, not making the model look
good.** Every rule in this plan exists to prevent us from fooling ourselves.
Where the model fails, the failure is the result — it gets reported, analyzed,
and used to decide what to build next.

This plan is *pre-registered*: metrics, baselines, calibration protocol, and
the parameter search space are fixed here **before** results are collected.
Changing them afterwards requires a new plan version with the change and its
reason logged in §9.

---

## 1. What is under test

The `cella_lib` wildfire model (`WildfireModel`): Alexandridis-style
stochastic CA — per-cell ignition probability `p0 × veg_factor × density`
with exponential wind/slope modifiers, burn duration in ticks, optional
lognormal spotting. Deterministic per seed; scored as seed ensembles.

Out of scope for v1: per-cell wind fields, fuel moisture, suppression. Each
is expected to be a *cause of measured failure* — the point of v1 is to
quantify how far the model gets without them.

## 2. Data

All sources are converted to the canonical scenario format (FORMATS.md).
Truth quality is part of the record: every score table carries the truth's
own `spatial_accuracy_m`.

| Tier | Scenarios | Truth cadence | Role |
|---|---|---|---|
| T0 | Six-fire pack (Bear, Chimney, Pier, Brattain, Ferguson, Buck) | daily | primary development + calibration set |
| T1 | Dogrib 2001 | final perimeter | cross-model benchmark (Prometheus F1 0.74, Cell2Fire 0.83) |
| T2 | PT-FireSprd (Portugal, 80 fires) | ~3-hourly | held-out generalization + junction/reversal stress |
| T3 | GOFER (28 CA fires) | hourly, ±1 km edges | arrival-time / growth-rate scoring; plume + terrain stress |
| T4 | Camp Fire 2018 (NIST points) | sub-hourly | spotting endgame |

**Holdout rule:** calibration may use at most 4 of the 6 T0 fires. The
remaining 2 T0 fires and every higher tier are *test-only*: no parameter may
be chosen, directly or indirectly, by looking at their scores. The T0 split
is fixed now, by alphabetical order — **calibration: Bear, Brattain, Buck,
Chimney; holdout: Ferguson, Pier.** Leave-one-fire-out cross-validation
within the calibration four is encouraged; re-splitting to move a
badly-scoring fire out of the holdout is forbidden.

## 3. Metrics (fixed)

Computed by `cella_lib/examples/wildfire_validate.rs` at each observation
time of the truth, as ensemble means:

1. **IoU (Jaccard)** and **Sørensen** of burned sets — the field standard;
   comparable to published baselines.
2. **Miss rate** (observed burned the model missed) and **false rate**
   (model burned that observation didn't) — the two failure directions IoU
   folds together.
3. **Arrival-time MAE** (hours) over cells burned in both, quantized to the
   observation cadence. Primary metric for T3+ where perimeter edges are
   coarse but timing is good.
4. **Burned-area-over-time curves** (recorded in every report JSON).

Ensembles: minimum 3 seeds during development, 10 for reported results;
report the mean and, for reported results, the min–max across seeds. Never
report a best seed.

## 4. Baselines (fixed) — the honesty floor

Every scored run includes, on identical truth:

- **Persistence**: ignition set never grows. Floor of floors — a model below
  this is destroying information.
- **Area-matched radial**: disc grown from the ignition set (chamfer
  distance), area matched to the observed area at every observation time.
  This null has *perfect area calibration* by construction, so beating it is
  evidence of genuine spatial skill: the model knows *where*, not just *how
  much*. Both are computed inside the harness on every run — they cannot be
  forgotten or omitted.

External bars, for context, not as targets to tune toward: neural-CA IoU
> 0.6 at 72 h (same T0 data, calibrated per fire with 10-day assimilation);
Cell2Fire F1 0.83 on Dogrib.

## 5. Starting-point record (2026-08-14, pre-calibration)

Textbook Alexandridis defaults (p0 = 0.58, burn_duration 5, first-guess
veg_factors, no spotting), all six T0 fires, 3 seeds, final IoU:

| Fire | model | persistence | radial null | arrival MAE (h) | verdict |
|---|---|---|---|---|---|
| Bear_2020 | 0.124 | 0.042 | 0.524 | 86.6 | loses to null |
| Brattain_2020 | 0.196 | 0.007 | 0.435 | 36.2 | loses to null |
| Buck_2017 | 0.143 | 0.138 | 0.616 | 185.8 | barely beats persistence |
| Chimney_2016 | **0.282** | 0.052 | 0.262 | 19.8 | **beats the null** |
| Ferguson_2018 | 0.287 | 0.003 | 0.361 | 103.0 | loses to null |
| Pier_2017 | 0.224 | 0.144 | 0.544 | 84.4 | loses to null |

**The uncalibrated model loses to the area-matched radial null on five of
six fires** — it systematically over-burns (false rates ~0.8+), so its
spatial pattern is currently worth less than "a disc of the right size".
The one exception, Chimney 2016, was the fastest wind-driven fire in the
set: where real spread is strongly directional, even the uncalibrated wind
kernel adds spatial signal. Slow fires (Buck: 185 h arrival MAE) are where
over-spread hurts most — consistent with the missing fuel-moisture /
suppression physics (§7). A quick probe also mapped a percolation cliff on
Bear: p0 0.10 dies (5k cells), p0 0.20 over-burns (212k), the observed 56k
sits in the narrow band between — per-fire sensitivity is high, which is
itself a finding about the model family.

## 6. Calibration protocol

- **Search space (pre-registered):** p0 ∈ [0.05, 0.6]; burn_duration ∈
  {1..20}; per-class veg_factor ∈ [0.1, 2.0] (6 classes); density from
  canopy cover on/off; spotting off, or (p_spot ∈ [0, 0.05], median ∈
  [1, 30] cells, σ ∈ [0.2, 1.0]). `steps_per_hour` is a declared constant
  (50/day), not searched.
- **Objective:** mean IoU across the calibration fires' observation series
  (not final-only — final-only rewards burning everything eventually).
- **Method:** coarse grid search, then coordinate descent; 3 seeds per
  evaluation; budget 500 evaluations per campaign, logged.
- **Two calibration modes, reported separately and labeled:**
  (a) *global* — one parameter set for all calibration fires (the honest
  headline number); (b) *per-fire* — what the published CA papers do; upper
  bound, reported only alongside (a), never alone.
- Calibrated parameter sets are written to
  `validation/results/calibrations/<campaign>.json` with the search log.

## 7. Failure-mode hypotheses (pre-registered)

What we expect to break, per tier — recorded now so the analysis is a check
against predictions, not a story fitted afterwards:

- T0: over-spread on low-wind days (no fuel moisture); stalls at late-fire
  containment lines (no suppression).
- T1 Dogrib: octagonal front-shape artifacts from the 8-neighbor lattice
  under the mid-run wind shift.
- T2 junction fires (Pedrógão): under-prediction of acceleration where
  fronts merge — a memoryless per-cell CA has no mechanism for it.
- T3 Creek: night-time under-spread and mis-timed plume-driven runs —
  growth decoupled from ambient wind.
- T3 Dixie: canyon wind channeling not representable by uniform wind.
- T4 Camp: lognormal spotting kernel cannot reproduce massed ember transport
  across the canyon; urban "non-burnable" fuels stall the front where the
  real fire accelerated.

Each scored tier gets a short written comparison against these predictions
in `validation/results/analysis/`.

## 8. Process rules

1. Any model or converter change → full re-run of every previously reported
   scenario before new numbers are quoted. Reports embed the git hash.
2. All scenarios in a tier get reported, including the bad ones. No
   cherry-picking fires, seeds, days, or metrics.
3. Truth is never edited to fit; suspected truth errors are logged in the
   scenario's provenance and taken upstream.
4. Engine changes remain gated by the existing correctness suite (FNV
   snapshots, thread-equivalence, ≥99 % coverage) — validation runs are not
   a substitute for it.
5. Negative results are kept: rejected calibrations, failed hypotheses, and
   worse-than-null configurations stay in the results directory and the
   analysis notes.

## 9. Plan changelog

- v1 (2026-08-14): initial plan. T0 split fixed (calibrate: Bear, Brattain,
  Buck, Chimney; holdout: Ferguson, Pier). Metrics, nulls, search space
  pre-registered. Starting-point record: uncalibrated model loses to the
  area-matched radial null on Bear 2020 (0.124 vs 0.524 final IoU).

## 10. Roadmap after v1

1. Calibration campaign on the T0 calibration set (§6), report on the T0
   holdout.
2. `rasterize_isochrones.py` (shared GDAL tool) → PT-FireSprd + GOFER truth;
   Dogrib inputs converter + observed-perimeter truth (Prometheus sample).
3. Arrival-time metrics against GOFER; growth-rate curves.
4. Input upgrades driven by measured failures, in whatever order the
   failures rank them: canopy-cover density, fuel moisture proxy, per-cell
   wind (WindNinja-downscaled), suppression masks.
