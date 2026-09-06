# Round 1 — 2026-08-15: which knobs close the gap?

_Score family: single-run mean IoU · 1-seed scans, 3 seeds to verify · calibration fires only · terms: [GLOSSARY.md](GLOSSARY.md)_

## What we knew before

Uncalibrated textbook parameters (p0 0.58, burn duration 5) lose to the
Circle on 5 of 6 fires (TEST_PLAN §5). The model burns everything
reachable (Bear ×8), so its map is worth less than a disc of the right
size. A p0 probe on Bear showed the percolation cliff: 0.10 dies, 0.20
over-burns, the real fire sits between.

## What we ran

| # | Question | Answer |
|---|---|---|
| [E1](01-e1-p0-burn-duration-scan.md) | Which (p0, duration) is best per fire? | 0.12–0.30 and 5–10; both knobs matter; Chimney beats the Circle. KEPT |
| [E2](02-e2-canopy-cover-density-layer.md) | Does canopy cover as a density layer help? | No (Buck −0.045). REJECTED |
| [E3](03-e3-weather-driven-daily-p0-schedule.md) | Does a p0 that follows daily weather help? | Temperature +0.05 on Buck, never hurts; rain fixes Bear, starves Buck. KEPT |
| [E4](04-e4-wider-veg-factor-spread.md) | A wider fuel-class spread? | No (Bear and Buck −0.06). REJECTED |
| [E5](05-e5-wind-gust-multiplier-2-4.md) | Multiply the daily wind ×2 or ×4? | No; it narrows the burn. REJECTED |
| [E6](06-e6-4-time-resolution.md) | 4× ticks per day with p0 ÷ 4? | Not a fair test; redone as E11. REJECTED as tested |
| [E7](07-e7-spotting-on.md) | Spotting on? | −0.1 to −0.2 everywhere. REJECTED |
| [E8](08-e8-ensemble-burn-probability-threshold.md) | Vote across seeds? | The union wins; the over-burn halo is the same on every seed. FINDING |

## What we know now

Verified results (3 seeds, `exp_verify.py`). Per-fire recipe = E1 best +
best E3 schedule. Global recipe = one setting for all fires (p0 0.22,
dur 5, temperature schedule), the honest headline mode.

| Fire | per-fire | global | Circle | arrival MAE (uncalibrated was) |
|---|---|---|---|---|
| Bear | 0.347 | 0.222 | 0.541 | 40 h (87 h) |
| Brattain | 0.344 | 0.320 | 0.450 | 98 h (36 h) |
| Buck | 0.448 | 0.362 | 0.670 | 43 h (186 h) |
| Chimney | **0.443** | **0.418** | 0.372 | 51 h (20 h) |

How to read it: mean IoU, higher is better; bold is where the model beats
the Circle. Scan-grade numbers held up under 3 seeds. The Circle still
wins 3 of 4: progress is real, but "knowing where to stop" (moisture,
suppression) remains the gap, as the pre-registered failure hypotheses
predicted (TEST_PLAN §7).

Engine findings (not score-related, arguably worth more):

1. **p0 write-after-attach footgun.** `params.p0` is baked into
   `derived.p_base` at attach ([wildfire.rs](../../cella_lib/src/wildfire.rs)
   `attach()`); writing `params.p0` afterwards is silently ignored. Wind
   params are read live per chunk, which hides the inconsistency. The
   first E3 run was a silent no-op because of it (all variants identical
   to control; caught because temperature schedules *cannot* leave
   results unchanged). Workaround used: `boxed_clone()` + `attach_model()`
   per window. Fixed in Round 3 by `set_p0`.
2. **Front-speed cap** (E6): a structural ceiling to keep in mind for
   sub-daily truth (PT-FireSprd, GOFER) even though daily scoring does
   not expose it.

## Still open after this round

1. A calibration campaign (TEST_PLAN §6) with informed priors: dur ∈
   {5, 10}, per-fire p0 ∈ [0.10, 0.35], per-class veg_factors free,
   spotting off.
2. Promote the weather schedule from harness hack to a `WildfireModel`
   fuel-moisture proxy input (§10.4): temperature-based first (never
   hurt), rain with care (over-suppresses under-burners).
3. File the p0-footgun engine fix.
4. What makes a fire *stop*? Nothing tried here does. → Round 3.

Skipped for good: spotting, gust multipliers, this canopy mapping, naive
time-resolution scaling.

## Configuration after this round

- **Per fire:** E1 (p0, dur) + E3 temperature schedule → 0.347 / 0.344 /
  0.448 / 0.443 (Bear / Brattain / Buck / Chimney).
- **Global:** p0 0.22, dur 5, temperature schedule → 0.222 / 0.320 /
  0.362 / 0.418.
- **Not yet scored:** the holdout pair. First holdout numbers arrive in
  E16c (Round 3).
