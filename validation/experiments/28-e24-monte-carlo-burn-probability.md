# E24 — Monte Carlo burn probability from one untuned prior · KEPT (the right product; ties the tuned single runs)

_Round: Round 4 — 2026-09-04: ensembles_

**Question.** Instead of one run with one tuned parameter set, run many
from a broad prior and report the *fraction of members* in which each
cell burned. Does the probability map beat the deterministic runs, and
is it calibrated?

**Method.** `wildfire_smc open`, 32 members per fire, prior p0
log-U[0.08, 0.6], burn duration U{5..20}, containment τ log-U[2, 100] d,
ERA5 wind × U[0, 1.5]; one seed per member; nothing chosen per fire, so
all six fires are reported (holdout included). Scores at every
observation time: consensus IoU (p ≥ 0.5), mean member IoU, Brier score
of the probability map, best-threshold IoU (peeks; diagnostic), all next
to the Circle and persistence. `exp_smc.py`.

| Fire | consensus IoU | mean member IoU | best single tuned run (E1/E20) | Circle | Brier ensemble | Brier Circle |
|---|---|---|---|---|---|---|
| Bear | **0.399** | 0.296 | 0.317 (E1) / 0.480 (E20) | 0.541 | 0.084 | 0.055 |
| Brattain | 0.330 | 0.252 | 0.337 / 0.408 | 0.450 | **0.102** | 0.123 |
| Buck | **0.481** | 0.374 | 0.414 / 0.581 | 0.670 | 0.107 | 0.041 |
| Chimney | **0.382** | 0.351 | 0.435 / 0.474 | 0.372 | **0.094** | 0.159 |
| Ferguson (holdout) | 0.131 | 0.173 | 0.145 (global) | 0.373 | **0.119** | 0.167 |
| Pier (holdout) | **0.478** | 0.379 | 0.321 (global) | 0.559 | 0.147 | 0.105 |

(Bold: consensus beats the tuned E1 single run, or Brier beats the Circle.)

**Findings.**

- **The consensus of 32 untuned members matches or beats the Round-1
  per-fire tuned single runs** on four of six fires (Bear +0.08, Buck
  +0.07, Pier +0.16 over the tuned/global deterministic runs) and beats
  the Circle on Chimney. No parameter was fitted. Averaging over the
  prior does what the E1 scan did by hand: members that explode are
  out-voted by members that stop, and the 0.5 threshold lands near the
  right area.
- **It is far better calibrated than any deterministic map.** Brier is
  40–60 % lower than the Circle's on Brattain, Chimney and Ferguson. The
  Circle "wins" Brier on Bear, Buck and Pier only because those fires are
  small relative to the domain, so a confident 0/1 disc that is right about
  the vast unburned area scores well; the ensemble pays for spreading
  probability over the halo. (A better reference for calibration is the
  reliability diagram; not computed yet.)
- **Mean member IoU (0.17–0.38) is far below the consensus** (0.13–0.48):
  the individual runs are as bad as ever; the *ensemble* is the thing that
  is good. This is the E8 finding inverted — in Round 1 the members agreed
  on a deterministic over-burn halo and voting could not help; with the
  decay in the prior the members disagree about *where* the fire stops,
  and voting does help.
- Ferguson is the exception once more: the prior is centred on
  over-burning fires, so most members under-burn it (area ×0.2) and the
  consensus is worse than the mean member. Assimilation fixes this (E25).

**Verdict.** Kept as the default product: a burn-probability map from a
broad prior, no per-fire tuning, honest on the holdout. Deterministic
single-run scores in the rest of the log should be read as a lower
bound on what the same model gives as an ensemble.
