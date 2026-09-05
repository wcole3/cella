# E33 — the noise floor: five seeds of the recommended ensemble · finding — sd ≤ 0.015 on five fires, 0.039 on Buck; a difference below the fire's sd is a tie

_Round: Round 5 — 2026-09-05: the methods themselves (Monte Carlo and GA operators)_

**Why.** Every comparison in Rounds 4–5 is one run against one run. E25d
had two extra replicates on the E25 configuration; E31 showed the same
configuration scoring 0.456 and 0.473 under the old engine. Before
judging ensemble size, operators or priors, the round needs the actual
run-to-run spread of the base configuration, per fire, from enough seeds
to quote a standard deviation. This is the bar for E32–E37: a delta
smaller than the fire's sd is reported as a tie.

**Method.** `exp_r5_noise.py`: the recommended configuration (assim, β 10,
σ 0.2, immigrants 0.2, containment only, M 32), five seeds (0–4; seed 0
is the E31 row), all six fires. The seed sets the prior draw, every
member's own dice and the daily containment rolls. Nothing else varies.

![Dot strip: five seeds per fire, mean, ±1 sd band and the Circle null](figures/e33-noise-floor.svg)

| Fire | seeds 0–4 (mean consensus IoU) | mean ± sd | Brier mean ± sd | Circle IoU / Brier |
|---|---|---|---|---|
| Bear | 0.478 0.479 0.493 0.455 0.489 | 0.479 ± 0.015 | 0.0510 ± 0.0030 | 0.541 / 0.0547 |
| Brattain | 0.415 0.416 0.415 0.410 0.421 | 0.416 ± 0.004 | 0.1090 ± 0.0042 | 0.450 / 0.1229 |
| Buck | 0.600 0.585 0.612 **0.527** 0.628 | 0.590 ± 0.039 | 0.0468 ± 0.0037 | 0.670 / 0.0408 |
| Chimney | 0.446 0.441 0.436 0.433 0.416 | 0.434 ± 0.012 | 0.1276 ± 0.0065 | 0.372 / 0.1591 |
| Ferguson* | 0.353 0.346 0.346 0.333 0.341 | 0.344 ± 0.007 | 0.1380 ± 0.0030 | 0.373 / 0.1675 |
| Pier* | 0.535 0.530 0.537 0.538 0.533 | 0.535 ± 0.003 | 0.1093 ± 0.0027 | 0.559 / 0.1047 |

(*holdout. Pooled rms sd 0.018; median 0.010. Effective sample size
after each learning step 28–31 of 32 on every fire and seed: the
population never collapsed.)

**Findings.**

- **The floor is small.** Five of six fires have sd ≤ 0.015 in mean IoU
  and ≤ 0.0065 in Brier. The 0.02 / 0.005 bars E31 borrowed from E25d
  were about right for IoU and slightly tight for Brier on Chimney.
- **Buck is the exception, and it is one seed.** Four seeds sit at
  0.585–0.628; seed 3 scores 0.527. Its per-day trace matches the others
  for eleven days and then falls (0.56 → 0.53) while the others rise
  (0.60 → 0.66–0.70): its members were all contained by day 12 while the
  real fire kept growing, and **a contained population cannot recover**.
  Immigrants take a fresh genome but inherit their parent's *state*,
  including the contained flag, so once every member has stopped there is
  no member left that can burn. This is a design gap in the filter, not a
  fluke of the dice: the same lock-in is visible, milder, in Bear seed 3
  (final-day 0.435 vs 0.464–0.493). Proposed E38: an immigrant resets its
  driver state (uncontained, p0 from its own genome), giving the
  population a way back when the observed fire out-runs its stop.
- Final-day IoU is noisier than the mean (Buck 0.417–0.566), as expected
  from a single day versus an average of thirty.

**Verdict.** Finding. The bar for the rest of Round 5 is the per-fire sd
above (use 0.02 as a round number on Bear/Chimney, 0.01 on Brattain/
Ferguson/Pier, 0.04 on Buck). Any claimed gain smaller than that on a
fire is a tie; a gain has to clear it on several fires at once.
