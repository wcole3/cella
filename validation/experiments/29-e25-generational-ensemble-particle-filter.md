# E25 — a fire that learns as it burns: particle filter with GA operators · KEPT — best honest forecast in the log, including on the holdout

_Round: Round 4 — 2026-09-04: ensembles_

**Question.** Let the ensemble *develop*: each day, score every member's
current burned set against the day's observed mask, keep the fit ones,
mutate their parameters, and keep simulating from their own state. Does
a fire that learns from yesterday's perimeter forecast tomorrow's better
than any fixed parameter set?

**Method.** `wildfire_smc assim`, 32 members, same untuned prior as E24.
At each observation: weights exp(β·IoU_member), systematic resampling,
children inherit the parent's grid (you cannot re-draw the past) with
log-normal parameter jitter (σ) and a fresh seed; optionally 20 % of
children are *immigrants* with fresh prior parameters (diversity floor);
optionally resample only every k-th observation (forecast horizon k
windows). **Every score at t_k is a forecast from the state assimilated
at t_{k−1}**; the mask at t_k is used only after being scored (TEST_PLAN
v1.4). Nothing chosen per fire; holdout reported. Runners `exp_smc.py`,
`exp_smc_imm.py`, `exp_smc_horizon.py`.

**One-window-ahead forecast skill, mean consensus IoU** (p ≥ 0.5):

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

(*holdout.) Brier of the immigrant config: Bear 0.051 (Circle 0.055),
Brattain 0.120 (0.123), Buck 0.044 (0.041), Chimney 0.126 (0.159),
Ferguson 0.146 (0.167), Pier 0.105 (0.105) — better or equal on five of six.

**Per-day trajectory, Bear (immigrant config):** open vs assimilated
consensus vs Circle — day 1: 0.45 / 0.45 / 0.54; day 3: 0.54 / **0.64** /
0.56; day 4: 0.50 / **0.62** / 0.59; day 8: 0.45 / 0.48 / 0.57; day 15+:
0.36 / 0.43 / 0.52. **Ferguson:** the open ensemble sits at 0.10–0.18 all
run; the assimilated one climbs 0.14 → 0.29 (day 5) → 0.42 (day 8) and
then tracks the Circle within 0.02, with p0 learned up from 0.24 to 0.38
and area ratio from ×0.2 to ×1.0.

**Findings.**

1. **Learning from yesterday's perimeter is worth +0.05 to +0.21 of
   forecast IoU over the same ensemble without learning**, on every fire,
   holdout included. Ferguson — the fire nothing in Rounds 1–3 could touch
   because the model under-burned it — goes from 0.13 to 0.34: the filter
   raises p0 and turns the decay off by itself. Both failure directions
   are corrected by the same mechanism.
2. **An untuned, honestly-forecasting ensemble matches the per-fire
   optimised single runs** (E20, which peeked at the whole series): Buck
   0.62 vs 0.58, Brattain 0.40 vs 0.41, Bear 0.47 vs 0.48, Pier 0.53 vs
   0.51. Only Chimney, the fast fire, is still better served by a tuned
   run (0.43 vs 0.47) — and the ensemble still beats the Circle there.
3. **Gap to the Circle is now 0.03–0.07 on five fires** (from 0.13–0.32
   in Round 1), zero fires lose to persistence, one beats the Circle. On
   Bear the assimilated ensemble *beats the Circle on days 2–4* (0.64 vs
   0.56) — the first time the model's spatial skill has beaten the
   area-matched null on a slow fire — and then loses it again as the
   observed fire stalls under containment and the ensemble, having learned
   a short τ, under-burns (area ×0.54). The late-fire stall is the same
   unmodelled mechanism as E21.
4. **Operators.** Sharper selection (β 30) helps the fast fires
   (Buck, Chimney) and hurts calibration (Brier up 0.02–0.04): fewer
   distinct members. Weaker mutation (σ 0.05) loses 0.01–0.03 everywhere:
   the population stops exploring. Immigrants give the best or equal
   consensus on three fires and the best Brier on four — the diversity
   floor is worth keeping. Recommended: β 10, σ 0.2, 20 % immigrants.
5. **Skill decays slowly with horizon.** Assimilating only every second
   observation (so half the scores are two-window forecasts) costs
   0.00–0.02; every third costs 0.01–0.05. A perimeter every other day is
   almost as good as a daily one.
6. **What the population learns is not what the offline search learned.**
   Posterior means: τ 5–20 d (E20 wanted 2.5–3.7), wind × 0.8–1.2 (E20
   turned wind off), p0 0.13–0.39 depending on the fire, burn duration
   10–16 (agrees). With the *state* corrected each day, the filter no
   longer needs a strong decay or a muted wind to compensate for early
   over-burn; the parameters move toward physically plausible values. The
   offline optimum was compensating for the model's initial-days error.

**Replicates (E25d, `exp_smc_reps.py`).** Three independent prior draws and member seeds of the recommended configuration, mean consensus IoU:

| Fire | seed 0 | seed 1 | seed 2 | mean ± sd |
|---|---|---|---|---|
| Bear | 0.473 | 0.461 | 0.471 | 0.468 ± 0.005 |
| Brattain | 0.400 | 0.404 | 0.407 | 0.404 ± 0.003 |
| Buck | 0.616 | 0.605 | 0.595 | 0.605 ± 0.009 |
| Chimney | 0.426 | 0.434 | 0.427 | 0.429 ± 0.004 |
| Ferguson | 0.345 | 0.342 | 0.360 | 0.349 ± 0.008 |
| Pier | 0.533 | 0.541 | 0.531 | 0.535 ± 0.004 |

Spread ≤ 0.02 on every fire: the result is not a lucky draw.

**Caveats.** Consensus threshold 0.5 fixed a priori; IoU-based weights are a choice
(a perimeter-distance likelihood may be better); resampling copies whole
grids (memory ∝ members × cells — fine to 64 members on these grids).

**Verdict.** Kept as the headline mode. This is the model in the form
an operations room would use it: yesterday's perimeter in, today's burn
probability out, no per-fire tuning, honest holdout. The E20 optimiser's
role is now diagnostic; forecasting is the ensemble's job.
