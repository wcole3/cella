# E28 — a containment-probability operator replaces the ad-hoc decay · KEPT — the published mechanism does the decay's job with no decay

_Round 4 (2026-09-04) · 32 members, one seed · all six fires incl. holdout · runner `exp_containment_op.py` → `wildfire_smc assim` · results `exp28_containment_op.json` · sources `research-decline-wind-suppression.md` §1 · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The decay (E16) was the log's biggest lever and its least
physical one. The US national fire simulator (FSim, Finney et al. 2011)
stops fires differently, and a 2025 generalized containment algorithm
fits the same idea to ICS-209 reports: each day a fire is caught with a probability that depends on
how fast it grew yesterday. We gave every ensemble member that rule, with
its two parameters learned by the filter like any other knob, and
switched the decay off. It does the decay's job on all six fires. The run
also showed a gain on Bear (+0.026); E31 later showed that gain was
run-to-run noise. The verdict rests on the mechanism, not the number.

**Question.** Can a published, interpretable stopping rule replace the
fitted decay at no cost?

**What we changed.** Once a day, each still-burning member computes its
growth `g = new cells / cells before today` and is contained with
probability `1 / (1 + e^(−(a + b·ln g)))`; contained members set p0 to 0
and keep their state. `a` and `b` are member parameters (prior a ∈ [−6,
−1], b ∈ [−2, −0.3]) that the filter mutates and selects. The operator is
*learned per fire from the perimeter*, not fitted to the ICS-209 record,
which stays available as an independent check. Configurations: `base`
(decay prior τ 2–100 d, as E25), `contain` (decay + operator), `contain,
τ off` (operator only).

**How we scored it.** Mean one-window-ahead consensus IoU and Brier, all
six fires, assimilating, β 10, σ 0.2, immigrants 0.2.

**Result.**

| config | Bear | Brattain | Buck | Chimney | Ferguson* | Pier* |
|---|---|---|---|---|---|---|
| base (decay only, E25) | 0.456 | 0.403 | 0.602 | 0.431 | 0.338 | 0.537 |
| decay + containment | 0.462 | 0.409 | 0.601 | 0.438 | 0.336 | 0.534 |
| **containment only (decay off)** | **0.482** | **0.413** | 0.599 | **0.438** | **0.344** | 0.529 |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 | 0.373 | 0.559 |

How to read it: forecast consensus IoU, higher is better; `*` is the
holdout pair; bold is the best row per fire. Brier: within ±0.003 of
base everywhere. Final-day consensus: Bear 0.420 → 0.454, Brattain
0.392 → 0.403, Chimney 0.394 → 0.415, others within ±0.02.

- The operator alone matches the decay everywhere and, in this run,
  scored higher on Bear (+0.026) and Brattain (+0.010). Buck and Pier are
  within noise (−0.003, −0.008; E25d spread was ≤ 0.02).
- **Both together add nothing over either alone.** Two stopping
  mechanisms compete for the same job.
- **What the population learns** (containment-only runs): p0 0.21–0.40,
  wind × 0.8–1.2, containment parameters mid-prior; the contained
  fraction rises through each run (Bear ≈ 60 % of members contained by
  day 12), which is the plateau in the observed area curves emerging
  from the rule rather than imposed by a clock.
- Ferguson and Chimney, the fast fires, are unchanged (0.34, 0.44): the
  operator correctly declines to contain fast-growing members; their problem is rate (E19, E26, E30), not stopping.

**What it means.** With the decay switched off, the ensemble's stopping
is a published, interpretable rule: a member that grew slowly yesterday
is likely to be caught today. The decay stays available for single-run
use, where a stochastic operator would need its own ensemble to mean
anything.

**Questions this raises.**

- Is the Bear gain real? → E31: no. Re-run through the generic engine,
  containment-only scores 0.478 against 0.486 for the decay; over six
  fires the two are a tie within noise.
- Does the learned daily containment rate match the ICS-209 record?
  Open; the data from E21 is now a check rather than a driver.
- Should the operator have a fuel-type term, as Finney's model does?
  Open.
- What happens when every member is contained and the fire is not? →
  E33 (one seed in thirty locks in), E38 (immigrant reset repairs it).

**Verdict.** Kept, and recommended over the decay: containment on, τ off
(at the time `prior.tau_days = [150, 150]` and `prior.containment` at its
default; today `SMC_CONTAIN=1 SMC_TAU_OFF=1`).

**Addendum (E31, 2026-09-05).** Re-run through the generic engine
([33](33-e31-generic-engine-replication.md)), containment-only scores
0.478 on Bear against 0.486 for the decay: the +0.026 above is
run-to-run noise, not a gain. The verdict stands on the mechanism (a
published stopping rule at no cost), not on the number.

**Later.** E31, E33, E34 (every filter run ends 100 % contained with a
flat tail), E38.
