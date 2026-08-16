# Wildfire Model — Research Experiment Log

This file tracks every experiment we run trying to improve the wildfire
model's real-world scores. One entry per experiment: what we tried, why,
what happened (numbers), and the verdict. Failed experiments stay in the
log — knowing what *doesn't* work is half the record (test plan §8 rule 5).

Ground rules for every entry, from [TEST_PLAN.md](TEST_PLAN.md):

- Experiments run on the **four calibration fires only** (Bear, Brattain,
  Buck, Chimney). The holdout pair (Ferguson, Pier) is never touched.
- "Mean IoU" below = mean overlap score over the observation series,
  t = 0 excluded (it is 1.0 by construction). Higher is better.
- Every number is quoted next to the Circle (area-matched radial null) on
  the same fire — beating the Circle is the bar that matters
  (see [ANALYSIS.md](ANALYSIS.md)).
- Scan-grade results use 1 seed (cheap, noisy); anything promoted gets a
  3-seed verification before it is believed.

**Tooling** (all committed):

- `cella_lib/examples/wildfire_experiment.rs` — copy of the scoring
  harness with experiment-only hooks the real harness must not have:
  `EXP_P0_SCALE` (per-day p0 multiplier schedule), `EXP_WIND_SCALE`
  (wind multiplier), `EXP_SEED_BASE` (seed offset for per-seed dumps).
- `validation/scripts/experiments/` — the runners, in run order:
  `exp_sweep.py` (E1), `make_p0_schedules.py` + `exp_variants.py`
  (E2–E5), `exp_spotting.py` (E6–E7), `exp_ensemble.py` (E8),
  `exp_verify.py` (3-seed verification). Outputs land in the gitignored
  `validation/results/experiments/`.

---

## Round 1 — 2026-08-15

Starting point: uncalibrated textbook parameters lose to the Circle on
5 of 6 fires (TEST_PLAN §5). Question: which levers close the gap?

### E1 — p0 × burn_duration scan · KEPT (the big lever)

Coarse grid: p0 ∈ {0.08, 0.12, 0.16, 0.22, 0.30} × burn_duration
∈ {2, 5, 10}, both inside the pre-registered search space (§6).
`exp_sweep.py`, 60 single-seed runs.

| Fire | best (p0, dur) | mean IoU | Circle | note |
|---|---|---|---|---|
| Bear | 0.12, 10 | 0.317 | 0.541 | was ~0.12 final uncalibrated |
| Brattain | 0.22, 10 | 0.337 | 0.450 | |
| Buck | 0.16, 5 | 0.414 | 0.670 | under-burns (×0.7) at best point |
| Chimney | 0.30, 5 | 0.435 | 0.372 | **beats the Circle** |

Findings: burn_duration matters as much as p0 (dur = 2 kills every fire
— the front outruns its own fuel); optimal p0 spans 0.12–0.30 across
fires, so one global setting costs ~0.07 mean IoU (global best
p0 = 0.22/dur = 5 averages 0.334 vs 0.376 for per-fire bests).

### E2 — canopy-cover density layer · REJECTED

Hypothesis: per-cell density from the HDF5 `230CC` layer adds the spatial
heterogeneity the model lacks. Mapping: forest cells 0.5 + 0.5·(CC/75),
grass kept at 1.0 (CC is *tree* cover; grass must not be punished).
`exp_variants.py::cc_density`.

Result: Bear +0.018, Brattain −0.017, Buck −0.045, Chimney −0.009.
Mixed-to-negative. Other mappings might work; this one doesn't.

### E3 — weather-driven daily p0 schedule · KEPT (first physics win)

Hypothesis: the model over-burns partly because p0 is constant in time —
real fires slow on rainy/cool days. Proxy schedules from ERA5 layers
(`make_p0_schedules.py`): rain `exp(-k·wet)` with 1-day carryover;
temperature `1 + 0.04·(T − mean)` clamped to [0.4, 1.6]; and the two
multiplied. Applied per wind window via `EXP_P0_SCALE`.

| Fire | control | rain k=1 | temp | rain×temp |
|---|---|---|---|---|
| Bear | 0.317 | 0.350 | 0.318 | **0.353** |
| Brattain | 0.337 | 0.344 | 0.339 | **0.346** |
| Buck | 0.414 | 0.332 | **0.460** | 0.356 |
| Chimney | 0.435 | 0.439 | 0.441 | **0.441** |

Findings: temperature never hurts and is worth +0.05 on Buck. Rain is
strong medicine — it collapsed Bear's over-burn from ×3.3 to ×0.3 area
(two rainy days carry real stopping information) but over-suppresses
Buck, which was already under-burning. Next step is folding a proper
fuel-moisture proxy into `WildfireModel` itself (roadmap §10.4) instead
of a harness hack.

### E4 — wider veg_factor spread · REJECTED

Grass 2.0 / GrassShrub 1.4 / Shrub 1.0 / TimberUnder 0.7 /
TimberLitter 0.3 / Slash 0.8 (vs the narrow defaults). Bear −0.061,
Buck −0.064, others ≈ flat. This particular spread is wrong; per-class
veg_factors stay in the calibration search space for the campaign.

### E5 — wind gust multiplier ×2 / ×4 · REJECTED

Hypothesis: ERA5 daily-mean winds (0.6–1.2 m/s) flatten the gusts;
at V ≈ 1 the wind kernel `exp(0.045·V)` ≈ 1.05 is nearly inert.
`EXP_WIND_SCALE`. Result: helps only Chimney at ×2 (+0.013), hurts Bear
(−0.02/−0.04), ≈ flat elsewhere. Consistently *narrows* the burn
(area ratios drop) — directionality without accuracy. The real fix is
per-cell wind (§10.4), not a scalar multiplier.

### E6 — 4× time resolution · REJECTED (as tested)

Hypothesis: 50 steps/day × 30 m caps front speed at 1.5 km/day; real
runs move 10–30 km/day. Tested steps ×4 with p0/4 (crude rate scaling).
Result: Bear −0.05, Brattain −0.02, Buck +0.03, Chimney −0.02. The p0/4
compensation is too crude near the percolation cliff; a fair test needs
a joint (steps, p0) scan. At daily truth cadence the over-burn error
dominates the too-slow error anyway.

### E7 — spotting on · REJECTED

Three settings inside the pre-registered spotting space (p_spot
0.001–0.005, median 5–20 cells). Every one neutral-to-catastrophic:
mid/far settings add ×2–4 area and cost 0.1–0.2 mean IoU on every fire.
Spotting amplifies exactly the failure we already have (over-burn). Not
worth revisiting until the model can *stop* — then it may help the fast
wind-driven fires.

### E8 — ensemble burn-probability threshold · finding, not a lever

Score "cells burned in ≥ q of 5 seeds" for q = 1..5 (`exp_ensemble.py`).
Union (q = 1) is best or tied on all four fires (+0.05 Buck, +0.02 Bear);
stricter voting only ever loses. Diagnosis: **the over-burn halo is
deterministic** — every seed agrees on it (percolation), while seeds
differ in which parts of the *real* burn they cover. Seed averaging can
therefore never remove false alarms. Union-of-seeds is a legitimate small
post-processing gain if we ever report ensemble masks.

### Verified results (3 seeds, `exp_verify.py`)

Per-fire recipe = E1 best + best E3 schedule. Global recipe = one setting
for all fires (p0 0.22, dur 5, temperature schedule) — the honest
headline mode.

| Fire | per-fire | global | Circle | arrival MAE (uncalibrated was) |
|---|---|---|---|---|
| Bear | 0.347 | 0.222 | 0.541 | 40 h (87 h) |
| Brattain | 0.344 | 0.320 | 0.450 | 98 h (36 h) |
| Buck | 0.448 | 0.362 | 0.670 | 43 h (186 h) |
| Chimney | **0.443** | **0.418** | 0.372 | 51 h (20 h) |

Scan-grade numbers held up under 3 seeds. The Circle still wins 3 of 4 —
progress is real but "knowing where to stop" (moisture, suppression)
remains the gap, as the pre-registered failure hypotheses predicted (§7).

### Engine findings (not score-related, arguably worth more)

1. **p0 write-after-attach footgun.** `params.p0` is baked into
   `derived.p_base` at attach ([wildfire.rs](../cella_lib/src/wildfire.rs)
   `attach()`); writing `params.p0` afterwards is silently ignored. Wind
   params are read live per chunk, which hides the inconsistency — the
   first E3 run was a silent no-op because of it (all variants identical
   to control; caught because temperature schedules *cannot* leave results
   unchanged). Workaround used: `boxed_clone()` + `attach_model()` per
   window. Engine task: either re-derive on param change or make the
   baked-in params impossible to write directly.
2. **Front-speed cap** (E6 above): a structural ceiling to keep in mind
   for sub-daily truth (PT-FireSprd, GOFER) even though daily scoring
   doesn't expose it.

### Round 1 conclusions → what to do next

1. Calibration campaign (§6) with informed priors: dur ∈ {5, 10},
   per-fire p0 ∈ [0.10, 0.35], per-class veg_factors free, spotting off.
2. Promote the weather schedule from harness hack to a `WildfireModel`
   fuel-moisture proxy input (§10.4) — temperature-based first (never
   hurt), rain with care (over-suppresses under-burners).
3. File the p0-footgun engine fix.
4. Skip: spotting, gust multipliers, this CC density mapping, naive
   time-resolution scaling.
