# Round 1 — 2026-08-15


Starting point: uncalibrated textbook parameters lose to the Circle on
5 of 6 fires (TEST_PLAN §5). Question: which levers close the gap?

## Verified results (3 seeds, `exp_verify.py`)


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

## Engine findings (not score-related, arguably worth more)


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

## Round 1 conclusions → what to do next


1. Calibration campaign (§6) with informed priors: dur ∈ {5, 10},
   per-fire p0 ∈ [0.10, 0.35], per-class veg_factors free, spotting off.
2. Promote the weather schedule from harness hack to a `WildfireModel`
   fuel-moisture proxy input (§10.4) — temperature-based first (never
   hurt), rain with care (over-suppresses under-burners).
3. File the p0-footgun engine fix.
4. Skip: spotting, gust multipliers, this CC density mapping, naive
   time-resolution scaling.

---
