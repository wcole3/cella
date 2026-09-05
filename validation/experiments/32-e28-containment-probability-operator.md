# E28 — a containment-probability operator replaces the ad-hoc decay · KEPT — the published mechanism does the decay's job with no decay

_Round: Round 4 — 2026-09-04: ensembles_

**Why.** The τ decay (E16) is the log's biggest lever and its least
physical one: a smooth exponential in time fitted to four fires, which
real containment data could not reproduce (E21). The way the field stops
simulated fires is different (research notes §1): FSim (Finney et al.
2011) terminates a fire each day with a *probability that depends on how
fast it grew*, from a statistical containment model; the 2025 generalized
containment algorithm fits the same idea to ICS-209 reports. Slow fires
get caught, fast ones do not.

**Implementation.** `WildfirePrior.containment = Some({a, b})` in the
library ensemble. Once a day (`end_of_day()`), each still-burning member
computes its growth `g = new cells / cells before today` and is contained
with probability `1 / (1 + e^(−(a + b·ln g)))`; contained members set p0
to 0 and keep their state. `a` and `b` are member parameters (prior
a ∈ [−6, −1], b ∈ [−2, −0.3]) that the filter mutates and selects like
the others — so the operator is *learned per fire from the perimeter*, not
fitted to the ICS-209 record, which stays available as an independent
check.

**Runs.** `exp_containment_op.py`: assimilating ensemble, 32 members,
β 10, σ 0.2, immigrants 0.2, all six fires (nothing chosen per fire):
`base` (decay prior τ 2–100 d, as E25), `contain` (decay + operator),
`contain, τ off` (operator only — the physical replacement).

| config | Bear | Brattain | Buck | Chimney | Ferguson* | Pier* |
|---|---|---|---|---|---|---|
| base (decay only, E25) | 0.456 | 0.403 | 0.602 | 0.431 | 0.338 | 0.537 |
| decay + containment | 0.462 | 0.409 | 0.601 | 0.438 | 0.336 | 0.534 |
| **containment only (decay off)** | **0.482** | **0.413** | 0.599 | **0.438** | **0.344** | 0.529 |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 | 0.373 | 0.559 |

(*holdout. Mean one-window-ahead consensus IoU. Brier: within ±0.003 of
base everywhere. Final-day consensus: Bear 0.420 → 0.454, Brattain
0.392 → 0.403, Chimney 0.394 → 0.415, others within ±0.02.)

**Findings.**

- **The physical mechanism replaces the ad-hoc one at no cost — and beats
  it on Bear (+0.026) and Brattain (+0.010).** Buck and Pier are within
  noise (−0.003, −0.008; E25d replicate spread was ≤ 0.02). With the
  decay switched off entirely, the ensemble's stopping is now a published,
  interpretable rule: a member that grew slowly yesterday is likely to be
  caught today.
- **Both together add nothing over either alone.** Two stopping
  mechanisms compete for the same job; the filter simply splits the work.
- **What the population learns** (containment-only runs): p0 0.21–0.40,
  wind × 0.8–1.2, and containment parameters that settle mid-prior; the
  contained fraction rises through each run (Bear ≈ 60 % of members
  contained by day 12), which is the plateau in the observed area curves
  emerging from the rule rather than being imposed by a clock.
- Ferguson and Chimney, the fast fires, are unchanged (0.34, 0.44): the
  operator correctly declines to contain fast-growing members, and their
  problem remains rate (E19/E26/E30), not stopping.

**Verdict.** Kept, and recommended over the decay: set
`prior.tau_days = [150, 150]` and `prior.containment` to the default. The
decay stays available for single-run use, where a stochastic operator
would need its own ensemble to be meaningful. Next: check the learned
daily containment rate against the ICS-209 percent-contained series
(the data from E21, now used as a check rather than a driver), and give
the operator a fuel-type term as Finney's model has.

**Addendum (E31, 2026-09-05).** Re-run through the generic engine
([33](33-e31-generic-engine-replication.md)), containment-only scores
0.478 on Bear against 0.486 for the decay: the +0.026 above is
run-to-run noise, not a gain. The verdict stands on the mechanism (a
published stopping rule at no cost), not on the number.
