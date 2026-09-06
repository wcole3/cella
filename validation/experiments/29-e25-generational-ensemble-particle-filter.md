# E25 — a fire that learns as it burns: particle filter with GA operators · KEPT — best honest forecast in the log, including on the holdout

_Round 4 (2026-09-04) · 32 members · all six fires incl. holdout · runners `exp_smc.py`, `exp_smc_imm.py`, `exp_smc_horizon.py`, `exp_smc_reps.py` → `wildfire_smc assim` · results `exp25b_smc_imm.json`, `exp25c_smc_horizon.json`, `exp25d_smc_reps.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Let the ensemble learn. Each day, score every member
against that day's observed mask, keep the ones that match, give their
knobs a small random nudge, add a few fresh draws, and let them keep
burning from where they are. Every score is a forecast made before the
day's mask is used, the way a fire camp would use last night's perimeter.
Forecast skill improves by 0.05–0.21 on every fire, holdout included.
Ferguson, which nothing in Rounds 1–3 could touch, goes from 0.13 to
0.34. On Bear the crowd beats the Circle on days 2–4. This is the
headline mode from here on.

**Question.** Does a fire that learns from yesterday's perimeter forecast
tomorrow's better than any fixed parameter set?

**What we changed.** `wildfire_smc assim`, 32 members, the E24 prior. At
each observation: weights exp(β × member IoU), systematic resampling;
children inherit the parent's grid (you cannot re-draw the past) with
log-normal knob jitter (σ) and a fresh seed; optionally 20 % of children
are *immigrants* with fresh prior knobs; optionally resample only every
k-th observation (a k-window forecast horizon). **Every score at t_k is a
forecast from the state assimilated at t_{k−1}**; the mask at t_k is used
only after being scored (TEST_PLAN v1.4).

**How we scored it.** Mean one-window-ahead consensus IoU (p ≥ 0.5) and
Brier, all six fires, beside the Circle and the best tuned single run.

**Result.**

| config | Bear | Brattain | Buck | Chimney | Ferguson* | Pier* |
|---|---|---|---|---|---|---|
| open (E24, no learning) | 0.399 | 0.330 | 0.481 | 0.382 | 0.131 | 0.478 |
| assim β10 σ0.2 | 0.463 | **0.412** | 0.595 | 0.427 | 0.344 | 0.529 |
| assim β30 σ0.2 | 0.448 | 0.409 | **0.620** | **0.437** | 0.334 | 0.528 |
| assim β10 σ0.05 | 0.457 | 0.397 | 0.562 | 0.429 | 0.341 | 0.535 |
| **assim β10 σ0.2 + 20 % immigrants** | **0.473** | 0.400 | 0.616 | 0.426 | **0.345** | 0.533 |
| … resample every 2nd obs (2-window forecast half the time) | 0.463 | 0.403 | 0.594 | 0.422 | 0.340 | **0.539** |
| … every 3rd | 0.463 | 0.386 | 0.571 | 0.418 | 0.336 | 0.538 |
| best tuned single run (E20 per-fire optimum) | 0.480 | 0.408 | 0.581 | 0.474 | 0.16 (transfer) | 0.51 (transfer) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 | 0.373 | 0.559 |

How to read it: forecast consensus IoU, higher is better; `*` marks the
holdout pair; bold is the best ensemble row per fire. The "best tuned
single run" row is a different score family (E20 saw the whole series)
and is context only. Brier of the immigrant config: Bear 0.051 (Circle
0.055), Brattain 0.120 (0.123), Buck 0.044 (0.041), Chimney 0.126
(0.159), Ferguson 0.146 (0.167), Pier 0.105 (0.105): better or equal on
five of six.

Per-day trajectory, Bear (immigrant config), open / assimilated
consensus / Circle: day 1: 0.45 / 0.45 / 0.54; day 3: 0.54 / **0.64** /
0.56; day 4: 0.50 / **0.62** / 0.59; day 8: 0.45 / 0.48 / 0.57; day 15+:
0.36 / 0.43 / 0.52. Ferguson: the open ensemble sits at 0.10–0.18 all
run; the assimilated one climbs 0.14 → 0.29 (day 5) → 0.42 (day 8) and
then tracks the Circle within 0.02, with p0 learned up from 0.24 to 0.38
and area ratio from ×0.2 to ×1.0.

1. **Learning from yesterday's perimeter is worth +0.05 to +0.21 of
   forecast IoU** over the same ensemble without learning, on every fire,
   holdout included. Ferguson goes from 0.13 to 0.34: the filter raises
   p0 and turns the decay off by itself. Both failure directions are
   corrected by the same mechanism.
2. **An untuned, honestly forecasting ensemble matches the per-fire
   optimised single runs** (E20, which peeked at the whole series): Buck
   0.62 vs 0.58, Brattain 0.40 vs 0.41, Bear 0.47 vs 0.48, Pier 0.53 vs
   0.51. Only Chimney, the fast fire, is still better served by a tuned
   run (0.43 vs 0.47), and the ensemble still beats the Circle there.
3. **Gap to the Circle is now 0.03–0.07 on five fires** (from 0.13–0.32
   in Round 1), zero fires lose to persistence, one beats the Circle. On
   Bear the assimilated ensemble *beats the Circle on days 2–4* (0.64 vs
   0.56), the first time the model's spatial skill has beaten the
   area-matched null on a slow fire, then loses it as the observed fire
   stalls and the ensemble, having learned a short τ, under-burns (area
   ×0.54). The late-fire stall is the same unmodelled mechanism as E21.
4. **Operators.** Sharper selection (β 30) helps the fast fires and hurts
   calibration (Brier up 0.02–0.04). Weaker mutation (σ 0.05) loses
   0.01–0.03 everywhere. Immigrants give the best or equal consensus on
   three fires and the best Brier on four. Recommended: β 10, σ 0.2,
   20 % immigrants.
5. **Skill decays slowly with horizon.** Assimilating every second
   observation costs 0.00–0.02; every third 0.01–0.05. A perimeter every
   other day is almost as good as a daily one.
6. **What the population learns is not what the offline search learned.**
   Posterior means: τ 5–20 d (E20 wanted 2.5–3.7), wind × 0.8–1.2 (E20
   turned wind off), p0 0.13–0.39 by fire, burn duration 10–16 (agrees).
   With the *state* corrected each day, the filter no longer needs a
   strong decay or a muted wind to compensate for early over-burn. The
   offline optimum was compensating for the model's initial-days error.

Replicates (E25d): three independent prior draws and member seeds of the
recommended configuration, mean consensus IoU:

| Fire | seed 0 | seed 1 | seed 2 | mean ± sd |
|---|---|---|---|---|
| Bear | 0.473 | 0.461 | 0.471 | 0.468 ± 0.005 |
| Brattain | 0.400 | 0.404 | 0.407 | 0.404 ± 0.003 |
| Buck | 0.616 | 0.605 | 0.595 | 0.605 ± 0.009 |
| Chimney | 0.426 | 0.434 | 0.427 | 0.429 ± 0.004 |
| Ferguson | 0.345 | 0.342 | 0.360 | 0.349 ± 0.008 |
| Pier | 0.533 | 0.541 | 0.531 | 0.535 ± 0.004 |

Spread ≤ 0.02 on every fire: the result is not a lucky draw.

**What it means.** This is the model in the form an operations room would
use it: yesterday's perimeter in, today's burn probability out, no
per-fire tuning, honest holdout. What remains is the late-fire stall (the
model still cannot stop *in place*) and the fast-fire speed problem.
Caveats: consensus threshold 0.5 fixed a priori; IoU-based weights are a
choice (a perimeter-distance likelihood may be better); resampling copies
whole grids (memory ∝ members × cells; fine to 64 members on these grids).

**Questions this raises.**

- Which operator carries the gain, and is 20 % immigrants really needed?
  → E34: the operators sit on a plateau; immigrants 0 is a tie once the
  containment operator does the stopping.
- Is the E25d spread the true noise floor? → E33: five seeds give sd
  ≤ 0.015 on five fires, 0.039 on Buck.
- Is the filter just a slow route to E20's knobs? → E36: no; it beats a
  GA fitted to the first three days on every fire.
- Can the stall be a mechanism instead of a clock? → E28: the containment
  operator. E38: when every member stops but the fire does not, fresh
  immigrants repair it.
- Does the prior decide the answer? → E35.

**Verdict.** Kept as the headline mode. The E20 optimiser's role is now
diagnostic; forecasting is the ensemble's job.

**Later.** E28 (containment operator replaces the decay in the prior),
E31 (replicated through the generic engine), E32–E38 (the methods round).
