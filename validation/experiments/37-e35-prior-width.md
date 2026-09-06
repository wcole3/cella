# E35 — prior width: narrow, broad, very broad · finding — the filter recovers from a prior that is too wide, not from one that is too narrow

_Round 5 (2026-09-05) · 1 seed · all six fires incl. holdout · runner `exp_r5_prior.py`, priors in `scripts/experiments/priors/` · results `exp35_prior.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The ranges the knobs are drawn from (the prior) were set by
hand in E24 (p0 0.08–0.6 log, burn duration 5–20, wind × 0–1.5). If the filter really learns from the perimeter, the prior
should matter little. We tried a narrow "informed" prior centred on what
earlier runs learned, and a very wide one. The narrow prior hurts, most
on Bear (−0.037), because it excludes where Bear actually settles. The
very wide prior is nearly free and better calibrated. Learning is
asymmetric: a too-wide prior gets searched; a too-narrow one can never be
escaped.

**Question.** Does the prior still decide the answer after learning?

**What we ran.** Three priors, same operators, seed 0, all six fires.
**narrow** = ±25 % around the E28/E31 posterior medians (p0 0.18–0.32
log, duration 11–17, wind × 0.5–0.9); **broad** = the E25 prior (E33 seed
0); **very broad** = p0 0.02–0.95 log, duration 2–60, wind × as broad.
Containment genes at their default ranges in all three.

**How we scored it.** Mean one-window-ahead consensus IoU and Brier,
judged against E33's per-fire sd; the learned p0 and wind × alongside.

![Dot strip per fire: narrow, broad and very broad priors against the E33 noise band and the Circle null](figures/e35-prior.svg)

**Result.**

| Fire | narrow | broad | very broad | E33 sd | learned p0 / wind ×: narrow · broad · very broad |
|---|---|---|---|---|---|
| Bear | **0.441** | 0.478 | 0.494 | 0.015 | 0.24/0.75 · 0.23/0.59 · 0.20/0.90 |
| Brattain | 0.408 | 0.415 | 0.415 | 0.004 | 0.25/0.73 · 0.27/0.80 · 0.19/0.65 |
| Buck | 0.619 | 0.600 | 0.614 | 0.039 | 0.25/0.72 · 0.22/0.67 · 0.14/0.79 |
| Chimney | 0.430 | 0.446 | 0.433 | 0.012 | 0.25/0.72 · 0.35/0.65 · 0.32/0.63 |
| Ferguson* | 0.343 | 0.353 | 0.350 | 0.007 | 0.25/0.73 · 0.35/0.94 · 0.44/0.81 |
| Pier* | 0.536 | 0.535 | **0.524** | 0.003 | 0.24/0.71 · 0.24/0.93 · 0.13/0.92 |

How to read it: forecast consensus IoU per prior, higher is better; bold
marks a loss beyond the fire's sd; `*` is the holdout pair. The last
column shows where the population ended up (median p0 and wind ×) under
each prior. Brier: very broad is best or equal on four fires (Bear
0.0482, Brattain 0.1021, Buck 0.0420, Chimney 0.1268, Pier 0.1063);
narrow is worst on Bear and Brattain.

- **The narrow prior hurts, and it hurts most where it was meant to
  help.** Bear loses 0.037 (2.5 sd), Chimney 0.016, Ferguson 0.010,
  Brattain 0.007; Buck's +0.019 is inside its sd. The narrow p0 range
  (0.18–0.32) excludes the region Bear settles in under the broad prior
  (p0 0.15–0.25 across the E33 seeds): day-3 consensus 0.48 versus 0.62.
  The narrow prior's learned knobs are the same on every fire (p0 ≈ 0.25,
  wind × ≈ 0.72): it did not learn, it stayed in the box. "±25 % around
  the posterior median" got one thing wrong: the *within-fire* posterior
  is that narrow, but the *between-fire* answers (p0 0.13–0.44) are not.
- **The very broad prior is nearly free.** Ties within the bar on Bear,
  Brattain, Buck, Chimney and Ferguson; a small real loss on Pier (−0.011
  against sd 0.003), where the population wandered to p0 0.13. Brier is
  *better* on four fires. The filter pulls a population from a ten-fold
  p0 range onto the fire within two or three days.
- **Learning is asymmetric.** Selection can only choose among what the
  prior offers; mutation moves a knob by σ × range per step and is
  clamped to the range. The cost of erring wide is a day of weaker
  forecasts; the cost of erring narrow is permanent.

**What it means.** Never tune the prior toward past posteriors: that is
the E20 mistake in a new costume. A prior has to cover the spread across
fires, not the spread within one.

**Questions this raises.**

- When cost forces a narrower prior, which knobs are safe to narrow? The
  ones posteriors agree on across fires (burn duration 10–16, containment
  slope); leave p0 and wind wide. Untested.

**Verdict.** Keep the E25 broad prior, or widen it when a new fire looks
unlike the six here.

**Later.** E36 (a three-day GA fit drives knobs to the box edges, the
same lesson from the other side).
